use std::{convert, io};
use std::fmt;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpStream};

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
const BLOCK_SIZE: usize = 16384;
type Block = Vec<u8>;

struct PeerConnection<'a> {
    metainfo: &'a Metainfo,
    stream: TcpStream,
    have: Vec<bool>,
    downloaded: Vec<Option<Block>>,
    am_i_choked: bool,
    am_i_interested: bool,
    are_they_choked: bool,
    are_they_interested: bool,
}

impl<'a> PeerConnection<'a> {
    fn connect(peer: &Peer, metainfo: &'a Metainfo) -> Result<PeerConnection<'a>, Error> {
        println!("Connecting to {}:{}", peer.ip, peer.port);
        let stream = try!(TcpStream::connect((peer.ip, peer.port)));
        let num_pieces = metainfo.info.pieces.len();
        let mut conn = PeerConnection {
            metainfo: metainfo,
            stream: stream,
            have: vec![false; num_pieces],
            downloaded: vec![None; num_pieces],
            am_i_choked: true,
            am_i_interested: false,
            are_they_choked: true,
            are_they_interested: false,
        };
        try!(conn.handshake(&metainfo.info_hash));
        loop {
            let message = try!(conn.receive_message());
            println!("Recieved: {:?}", message);
            try!(conn.process(message));
        }
        Ok(conn)
    }

    fn handshake(&mut self, info_hash: &[u8]) -> Result<(), Error> {
        try!(self.send_handshake(info_hash));
        try!(self.receive_handshake());
        Ok(())
    }

    fn send_handshake(&mut self, info_hash: &[u8]) -> Result<(), Error> {
        let mut message = vec![];
        message.push(PROTOCOL.len() as u8);
        message.extend(PROTOCOL.as_bytes().iter().cloned());
        message.extend((&[0; 8]).iter().cloned());
        message.extend(info_hash.iter().cloned());
        message.extend("-TZ-0000-00000000001".as_bytes().iter().cloned());
        try!(self.stream.write_all(&message));
        Ok(())
    }

    fn receive_handshake(&mut self) -> Result<(), Error> {
        let pstrlen = try!(self.read_n(1));
        let pstr = try!(self.read_n(pstrlen[0] as usize));
        let reserved = try!(self.read_n(8));
        let info_hash = try!(self.read_n(20));
        let peer_id = try!(self.read_n(20));
        Ok(())
    }

    fn send_message(&mut self, message: Message) -> Result<(), Error> {
        println!("Sending: {:?}", message);
        try!(self.stream.write_all(&message.encode()));
        Ok(())
    }

    fn receive_message(&mut self) -> Result<Message, Error> {
        let message_size = convert_big_endian_to_integer(&try!(self.read_n(4)));
        if message_size > 0 {
            let message = try!(self.read_n(message_size));
            Ok(Message::new(&message[0], &message[1..]))
        } else {
            Ok(Message::KeepAlive)
        }
    }

    fn read_n(&mut self, bytes_to_read: usize) -> Result<Vec<u8>, Error> {
        let mut buf = vec![];
        let bytes_read = (&mut self.stream).take(bytes_to_read as u64).read_to_end(&mut buf);
        match bytes_read {
            Ok(n) if n == bytes_to_read  => Ok(buf),
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
                self.have[have_index] = true;
                try!(self.send_interested());
            },
            Message::Unchoke => {
                if self.am_i_choked {
                    self.am_i_choked = false;
                    try!(self.request_next_piece());
                }
            }
            Message::Piece(index, begin, data) => {
                self.downloaded[index] = Some(data);
                try!(self.request_next_piece());
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

    fn request_next_piece(&mut self) -> Result<(), Error> {
        match self.next_piece_to_request() {
            Some(piece_index) => self.send_request(piece_index),
            None => Ok(())
        }
    }

    fn next_piece_to_request(&self) -> Option<usize> {
        for i in 0..self.downloaded.len() {
            if self.downloaded[i].is_none() {
                return Some(i)
            }
        }
        println!("Done downloading file!");
        None
    }

    fn send_request(&mut self, piece: usize) -> Result<(), Error> {
        let num_pieces = self.downloaded.len();
        let request_size = if piece == num_pieces - 1 {
            self.metainfo.info.length as usize - (self.metainfo.info.piece_length as usize * (num_pieces - 1))
        } else {
            BLOCK_SIZE
        };
        self.send_message(Message::Request(piece, 0, request_size))
    }
}

enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(usize),
    Bitfield(Vec<u8>),
    Request(usize, usize, usize),
    Piece(usize, usize, Block),
    Cancel,  // TODO add params
    Port,    // TODO add params
}

impl Message {
    fn new(id: &u8, body: &[u8]) -> Message {
        match *id {
            0 => Message::Choke,
            1 => Message::Unchoke,
            2 => Message::Interested,
            3 => Message::NotInterested,
            4 => Message::Have(convert_big_endian_to_integer(body)),
            5 => Message::Bitfield(body.to_owned()),
            6 => {
                let index = convert_big_endian_to_integer(&body[0..4]);
                let begin = convert_big_endian_to_integer(&body[4..8]);
                let offset = convert_big_endian_to_integer(&body[8..12]);
                Message::Request(index, begin, offset)
            },
            7 => {
                let index = convert_big_endian_to_integer(&body[0..4]);
                let begin = convert_big_endian_to_integer(&body[4..8]);
                let data = body[8..].to_owned();
                Message::Piece(index, begin, data)
            },
            8 => Message::Cancel,
            9 => Message::Port,
            _ => panic!("Bad message id: {}", id)
        }
    }

    fn encode(self) -> Vec<u8> {
        let mut payload = vec![];
        match self {
            Message::KeepAlive => {},
            Message::Choke => payload.push(0),
            Message::Unchoke => payload.push(1),
            Message::Interested => payload.push(2),
            Message::NotInterested => payload.push(3),
            Message::Have(index) => {
                payload.push(4);
                payload.extend(convert_usize_to_bytes(index).iter().cloned());
            },
            Message::Bitfield(bytes) => {
                payload.push(5);
                payload.extend(bytes);
            },
            Message::Request(index, begin, amount) => {
                payload.push(6);
                payload.extend(convert_usize_to_bytes(index).iter().cloned());
                payload.extend(convert_usize_to_bytes(begin).iter().cloned());
                payload.extend(convert_usize_to_bytes(amount).iter().cloned());
            },
            Message::Piece(index, begin, data) => {
                payload.push(6);
                payload.extend(convert_usize_to_bytes(index).iter().cloned());
                payload.extend(convert_usize_to_bytes(begin).iter().cloned());
                payload.extend(data);
            },
            Message::Cancel => payload.push(8),
            Message::Port => payload.push(9),
        };

        // prepend size
        let mut size = convert_usize_to_bytes(payload.len());
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
             Message::Request(ref index, ref begin, ref offset) => write!(f, "Request({}, {}, {})", index, begin, offset),
             Message::Piece(ref index, ref begin, ref data) => write!(f, "Piece({}, {}, size={})", index, begin, data.len()),
             Message::Cancel => write!(f, "Cancel"),
             Message::Port => write!(f, "Port"),
        }
    }
}

const BYTE_0: usize = 256 * 256 * 256;
const BYTE_1: usize = 256 * 256;
const BYTE_2: usize = 256;
const BYTE_3: usize = 1;

fn convert_big_endian_to_integer(bytes: &[u8]) -> usize {
    bytes[0] as usize * BYTE_0 +
    bytes[1] as usize * BYTE_1 +
    bytes[2] as usize * BYTE_2 +
    bytes[3] as usize * BYTE_3
}

fn convert_usize_to_bytes(integer: usize) -> Vec<u8> {
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
