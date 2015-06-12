use std::{convert, io};
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::net::{Ipv4Addr, TcpStream};

use hash::{calculate_sha1, Sha1};
use metainfo::Metainfo;
use tracker_response::Peer;

pub fn download(info: &Metainfo, peers: &[Peer]) {
    for peer in peers {
        if peer.ip != Ipv4Addr::new(207, 251, 103, 46) {
            match PeerConnection::connect(peer, &info) {
                Ok(_) => println!("Peer done"),
                Err(e) => println!("Error: {:?}", e)
            }
        }
    }
}

const PROTOCOL: &'static str = "BitTorrent protocol";
const BLOCK_SIZE: u32 = 16384;

struct Download<'a> {
    metainfo: &'a Metainfo,
    pieces:   Vec<Piece>,
    file:     File,
}

impl<'a> Download<'a> {
    fn new(metainfo: &Metainfo) -> Result<Download, Error> {
        let file_length = metainfo.info.length;
        let piece_length = metainfo.info.piece_length;
        let num_pieces = metainfo.info.num_pieces;

        // create pieces
        let mut pieces = vec![];
        for i in 0..num_pieces {
            let len = if i < (num_pieces - 1) {
                piece_length
            } else {
                (file_length - (piece_length as u64 * (num_pieces as u64 - 1))) as u32
            };
            pieces.push(Piece::new(i, len, piece_length, metainfo.info.pieces[i as usize].clone()));
        }

        // create file
        let mut file = try!(File::create(&metainfo.info.name));
        try!(file.set_len(metainfo.info.length));

        Ok(Download {
            metainfo: metainfo,
            pieces:   pieces,
            file:     file,
        })
    }

    fn store(&mut self, piece_index: u32, block_index: u32, data: Vec<u8>) -> Result<(), Error> {
        let piece = &mut self.pieces[piece_index as usize];
        piece.store(&mut self.file, block_index, data)
        // TODO Detect if file is complete
    }

    fn next_block_to_request(&self, peer_has_pieces: &[bool]) -> Option<(u32, u32, u32)> {
        for piece in self.pieces.iter() {
            if peer_has_pieces[piece.index as usize] {
                match piece.next_block_to_request() {
                    Some(block) => {
                        return Some((piece.index, block.index, block.length))
                    },
                    None => {}
                }
            }
        }
        None
    }
}

struct Piece {
    index:  u32,
    length: u32,
    piece_length: u32,
    hash:   Sha1,
    blocks: Vec<Block>,
}

impl Piece {
    fn new(index: u32, length: u32, piece_length: u32, hash: Sha1) -> Piece {
        // create blocks
        let mut blocks = vec![];
        let num_blocks = (length as f64 / BLOCK_SIZE as f64).ceil() as u32;
        for i in 0..num_blocks {
            let len = if i < (num_blocks - 1) {
                BLOCK_SIZE
            } else {
                length - (BLOCK_SIZE * (num_blocks - 1))
            };
            blocks.push(Block::new(i, len));
        }

        Piece {
            index:  index,
            length: length,
            piece_length: piece_length,
            hash:   hash,
            blocks: blocks,
        }
    }

    fn store(&mut self, file: &mut File, block_index: u32, data: Vec<u8>) -> Result<(), Error> {
        {
            let block = &mut self.blocks[block_index as usize];
            block.data = Some(data);
        }

        if self.is_complete() {
            // concatenate data from blocks together
            let mut data = vec![];
            for block in self.blocks.iter() {
                data.extend(block.data.clone().unwrap());
            }

            // validate that piece data matches SHA1 hash
            if self.hash == calculate_sha1(&data) {
                println!("Piece {} is complete and correct", self.index);
                let offset = self.index as u64 * self.piece_length as u64;
                println!("Writing {} bytes to file at offset {}", data.len(), offset);
                try!(file.seek(io::SeekFrom::Start(offset)));
                try!(file.write_all(&data));
            } else {
                println!("Piece is corrupt, deleting downloaded piece data!");
                for block in self.blocks.iter_mut() {
                    block.data = None;
                }
            }
        }
        Ok(())
    }

    fn next_block_to_request(&self) -> Option<&Block> {
        for block in self.blocks.iter() {
            if block.data.is_none() {
                return Some(block)
            }
        }
        None
    }

    fn is_complete(&self) -> bool {
        for block in self.blocks.iter() {
            if block.data.is_none() {
                return false
            }
        }
        true
    }
}

struct Block {
    index:  u32,
    length: u32,
    data:   Option<Vec<u8>>,
}

impl Block {
    fn new(index: u32, length: u32) -> Block {
        Block {
            index:  index,
            length: length,
            data:   None,
        }
    }
}

struct PeerConnection<'a> {
    metainfo: &'a Metainfo,
    stream: TcpStream,
    have: Vec<bool>,
    download: Download<'a>,
    am_i_choked: bool,
    am_i_interested: bool,
    are_they_choked: bool,
    are_they_interested: bool,
}

impl<'a> PeerConnection<'a> {
    fn connect(peer: &Peer, metainfo: &'a Metainfo) -> Result<PeerConnection<'a>, Error> {
        println!("Connecting to {}:{}", peer.ip, peer.port);
        let stream = try!(TcpStream::connect((peer.ip, peer.port)));
        let num_pieces = metainfo.info.num_pieces;
        let download = try!(Download::new(metainfo));
        let mut conn = PeerConnection {
            metainfo: metainfo,
            stream: stream,
            have: vec![false; num_pieces as usize],
            download: download,
            am_i_choked: true,
            am_i_interested: false,
            are_they_choked: true,
            are_they_interested: false,
        };
        try!(conn.run());
        Ok(conn)
    }

    fn run(&mut self) -> Result<(), Error> {
        try!(self.send_handshake());
        try!(self.receive_handshake());
        loop {
            let message = try!(self.receive_message());
            println!("Recieved: {:?}", message);
            try!(self.process(message));
        }
        Ok(())
    }

    fn send_handshake(&mut self) -> Result<(), Error> {
        let mut message = vec![];
        message.push(PROTOCOL.len() as u8);
        message.extend(PROTOCOL.bytes());
        message.extend(vec![0; 8].into_iter());
        message.extend(self.metainfo.info_hash.iter().cloned());
        message.extend("-TZ-0000-00000000001".bytes());
        try!(self.stream.write_all(&message));
        Ok(())
    }

    fn receive_handshake(&mut self) -> Result<(), Error> {
        let pstrlen = try!(self.read_n(1));
        let pstr = try!(self.read_n(pstrlen[0] as u32));
        let reserved = try!(self.read_n(8));
        let info_hash = try!(self.read_n(20));
        let peer_id = try!(self.read_n(20));
        Ok(())
    }

    fn send_message(&mut self, message: Message) -> Result<(), Error> {
        println!("Sending: {:?}", message);
        try!(self.stream.write_all(&message.serialize()));
        Ok(())
    }

    fn receive_message(&mut self) -> Result<Message, Error> {
        let message_size = bytes_to_u32(&try!(self.read_n(4)));
        if message_size > 0 {
            let message = try!(self.read_n(message_size));
            Ok(Message::new(&message[0], &message[1..]))
        } else {
            Ok(Message::KeepAlive)
        }
    }

    fn read_n(&mut self, bytes_to_read: u32) -> Result<Vec<u8>, Error> {
        let mut buf = vec![];
        let bytes_read = (&mut self.stream).take(bytes_to_read as u64).read_to_end(&mut buf);
        match bytes_read {
            Ok(n) if n == bytes_to_read as usize => Ok(buf),
            Ok(_)  => Err(Error::NotEnoughData),
            Err(e) => try!(Err(e))
        }
    }

    fn process(&mut self, message: Message) -> Result<(), Error>{
        match message {
            Message::KeepAlive => {},
            Message::Bitfield(bytes) => {
                for have_index in 0..self.have.len() {
                    let bytes_index = have_index / 8;
                    let index_into_byte = have_index % 8;
                    let byte = bytes[bytes_index];
                    let value = (byte & (1 << (7 - index_into_byte))) != 0;
                    self.have[have_index] = value;
                };
                try!(self.send_interested());
            },
            Message::Have(have_index) => {
                self.have[have_index as usize] = true;
                try!(self.send_interested());
            },
            Message::Unchoke => {
                if self.am_i_choked {
                    self.am_i_choked = false;
                    try!(self.request_next_block());
                }
            }
            Message::Piece(piece_index, offset, data) => {
                let block_index = offset / BLOCK_SIZE;
                try!(self.download.store(piece_index, block_index, data));
                try!(self.request_next_block());
            }
            _ => panic!("Need to process message: {:?}", message)
        };
        Ok(())
    }

    fn send_interested(&mut self) -> Result<(), Error> {
        if self.am_i_interested == false {
            self.am_i_interested = true;
            try!(self.send_message(Message::Interested));
        }
        Ok(())
    }

    fn request_next_block(&mut self) -> Result<(), Error> {
        match self.download.next_block_to_request(&self.have) {
            Some((piece_index, block_index, block_length)) => {
                let offset = block_index * BLOCK_SIZE;
                self.send_message(Message::Request(piece_index, offset, block_length))
            },
            None => {
                println!("We've downloaded all the pieces we can from this peer.");
                Ok(())
            }
        }
    }
}

enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(u32),
    Bitfield(Vec<u8>),
    Request(u32, u32, u32),
    Piece(u32, u32, Vec<u8>),
    Cancel,  // TODO Add params
    Port,    // TODO Add params
}

impl Message {
    fn new(id: &u8, body: &[u8]) -> Message {
        match *id {
            0 => Message::Choke,
            1 => Message::Unchoke,
            2 => Message::Interested,
            3 => Message::NotInterested,
            4 => Message::Have(bytes_to_u32(body)),
            5 => Message::Bitfield(body.to_owned()),
            6 => {
                let index = bytes_to_u32(&body[0..4]);
                let offset = bytes_to_u32(&body[4..8]);
                let length = bytes_to_u32(&body[8..12]);
                Message::Request(index, offset, length)
            },
            7 => {
                let index = bytes_to_u32(&body[0..4]);
                let offset = bytes_to_u32(&body[4..8]);
                let data = body[8..].to_owned();
                Message::Piece(index, offset, data)
            },
            8 => Message::Cancel,
            9 => Message::Port,
            _ => panic!("Bad message id: {}", id)
        }
    }

    fn serialize(self) -> Vec<u8> {
        let mut payload = vec![];
        match self {
            Message::KeepAlive => {},
            Message::Choke => payload.push(0),
            Message::Unchoke => payload.push(1),
            Message::Interested => payload.push(2),
            Message::NotInterested => payload.push(3),
            Message::Have(index) => {
                payload.push(4);
                payload.extend(u32_to_bytes(index).into_iter());
            },
            Message::Bitfield(bytes) => {
                payload.push(5);
                payload.extend(bytes);
            },
            Message::Request(index, offset, amount) => {
                payload.push(6);
                payload.extend(u32_to_bytes(index).into_iter());
                payload.extend(u32_to_bytes(offset).into_iter());
                payload.extend(u32_to_bytes(amount).into_iter());
            },
            Message::Piece(index, offset, data) => {
                payload.push(6);
                payload.extend(u32_to_bytes(index).into_iter());
                payload.extend(u32_to_bytes(offset).into_iter());
                payload.extend(data);
            },
            Message::Cancel => payload.push(8),
            Message::Port => payload.push(9),
        };

        // prepend size
        let mut size = u32_to_bytes(payload.len() as u32);
        size.extend(payload);
        size
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
             Message::KeepAlive => write!(f, "KeepAlive"),
             Message::Choke => write!(f, "Choke"),
             Message::Unchoke => write!(f, "Unchoke"),
             Message::Interested => write!(f, "Interested"),
             Message::NotInterested => write!(f, "NotInterested"),
             Message::Have(ref index) => write!(f, "Have({})", index),
             Message::Bitfield(ref bytes) => write!(f, "Bitfield({:?})", bytes),
             Message::Request(ref index, ref offset, ref length) => write!(f, "Request({}, {}, {})", index, offset, length),
             Message::Piece(ref index, ref offset, ref data) => write!(f, "Piece({}, {}, size={})", index, offset, data.len()),
             Message::Cancel => write!(f, "Cancel"),
             Message::Port => write!(f, "Port"),
        }
    }
}

const BYTE_0: u32 = 256 * 256 * 256;
const BYTE_1: u32 = 256 * 256;
const BYTE_2: u32 = 256;
const BYTE_3: u32 = 1;

fn bytes_to_u32(bytes: &[u8]) -> u32 {
    bytes[0] as u32 * BYTE_0 +
    bytes[1] as u32 * BYTE_1 +
    bytes[2] as u32 * BYTE_2 +
    bytes[3] as u32 * BYTE_3
}

fn u32_to_bytes(integer: u32) -> Vec<u8> {
    let mut rest = integer;
    let first = rest / BYTE_0;
    rest -= first * BYTE_0;
    let second = rest / BYTE_1;
    rest -= second * BYTE_1;
    let third = rest / BYTE_2;
    rest -= third * BYTE_2;
    let fourth = rest;
    vec![first as u8, second as u8, third as u8, fourth as u8]
}

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    NotEnoughData,
}

impl convert::From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}
