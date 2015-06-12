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
const BLOCK_SIZE: u32 = 16384;
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
                    try!(self.request_next_piece());
                }
            }
            Message::Piece(index, begin, data) => {
                self.downloaded[index as usize] = Some(data);
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

    fn next_piece_to_request(&self) -> Option<u32> {
        for i in 0..self.downloaded.len() {
            if self.downloaded[i].is_none() {
                return Some(i as u32)
            }
        }
        println!("Done downloading file!");
        None
    }

    fn send_request(&mut self, piece: u32) -> Result<(), Error> {
        let num_pieces = self.downloaded.len() as u32;
        let request_size = if piece == num_pieces - 1 {
            self.metainfo.info.length - (self.metainfo.info.piece_length * (num_pieces - 1))
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
    Have(u32),
    Bitfield(Vec<u8>),
    Request(u32, u32, u32),
    Piece(u32, u32, Block),
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
            4 => Message::Have(bytes_to_u32(body)),
            5 => Message::Bitfield(body.to_owned()),
            6 => {
                let index = bytes_to_u32(&body[0..4]);
                let begin = bytes_to_u32(&body[4..8]);
                let offset = bytes_to_u32(&body[8..12]);
                Message::Request(index, begin, offset)
            },
            7 => {
                let index = bytes_to_u32(&body[0..4]);
                let begin = bytes_to_u32(&body[4..8]);
                let data = body[8..].to_owned();
                Message::Piece(index, begin, data)
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
            Message::Request(index, begin, amount) => {
                payload.push(6);
                payload.extend(u32_to_bytes(index).into_iter());
                payload.extend(u32_to_bytes(begin).into_iter());
                payload.extend(u32_to_bytes(amount).into_iter());
            },
            Message::Piece(index, begin, data) => {
                payload.push(6);
                payload.extend(u32_to_bytes(index).into_iter());
                payload.extend(u32_to_bytes(begin).into_iter());
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
             Message::Request(ref index, ref begin, ref offset) => write!(f, "Request({}, {}, {})", index, begin, offset),
             Message::Piece(ref index, ref begin, ref data) => write!(f, "Piece({}, {}, size={})", index, begin, data.len()),
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
