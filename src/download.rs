use std::{convert, io};
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

const BLOCK_SIZE: usize = 16384;

struct PeerConnection {
    stream: TcpStream,
    have: Vec<bool>,
    am_i_choked: bool,
    am_i_interested: bool,
    are_they_choked: bool,
    are_they_interested: bool,
}

impl PeerConnection {
    fn connect(peer: &Peer, metainfo: &Metainfo) -> Result<PeerConnection, Error> {
        println!("Connecting to {}:{}", peer.ip, peer.port);
        let stream = try!(TcpStream::connect((peer.ip, peer.port)));
        let mut conn = PeerConnection {
            stream: stream,
            have:   vec![false; metainfo.info.pieces.len()],
            am_i_choked: true,
            am_i_interested: false,
            are_they_choked: true,
            are_they_interested: false,
        };
        try!(conn.handshake(&metainfo.info_hash));
        loop {
            let message = try!(conn.receive_message());
            println!("{:?}", message);
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
        message.push(19);
        message.extend("BitTorrent protocol".as_bytes().iter().cloned());
        message.push(0);
        message.push(0);
        message.push(0);
        message.push(0);
        message.push(0);
        message.push(0);
        message.push(0);
        message.push(0);
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

    fn receive_message(&mut self) -> Result<Message, Error> {
        let message_size = convert_big_endian_to_integer(&try!(self.read_n(4)));
        if message_size > 0 {
            let message = try!(self.read_n(message_size));
            let message_id = &message[0];
            let message_body = &message[1..];
            Ok(Message::new(message_id, message_body))
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
                self.send_interested();
            },
            Message::Have(have_index) => {
                self.have[have_index] = true;
                self.send_interested();
            },
            Message::Unchoke => {
                if self.am_i_choked {
                    self.am_i_choked = false;
                    let piece_index = self.next_piece_to_request();
                    self.send_request(piece_index);
                }
            }
            _ => panic!("Need to process message: {:?}", message)
        };
        Ok(())
    }
    fn send_interested(&mut self) -> Result<(), Error> {
        if self.am_i_interested == false {
            let mut message = vec![];
            message.push(0);
            message.push(0);
            message.push(0);
            message.push(1);
            message.push(2);
            try!(self.stream.write_all(&message));
            self.am_i_interested = true;
        }
        Ok(())
    }

    fn next_piece_to_request(&self) -> usize {
        0
    }

    fn send_request(&mut self, piece: usize) -> Result<(), Error> {
        let mut message = vec![];
        message.push(0);
        message.push(0);
        message.push(0);
        message.push(13);
        message.push(6);
        message.extend(convert_usize_to_bytes(piece).iter().cloned());
        message.extend(convert_usize_to_bytes(0).iter().cloned());
        message.extend(convert_usize_to_bytes(BLOCK_SIZE).iter().cloned());
        try!(self.stream.write_all(&message));
        Ok(())
    }
}

#[derive(Debug)]
enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(usize),
    Bitfield(Vec<u8>),
    Request, // TODO add params
    Piece,   // TODO add params
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
            6 => Message::Request,
            7 => Message::Piece,
            8 => Message::Cancel,
            9 => Message::Port,
            _ => panic!("Bad message id: {}", id)
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
