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

struct PeerConnection {
    stream: TcpStream,
    have: Vec<bool>,
}

impl PeerConnection {
    fn connect(peer: &Peer, metainfo: &Metainfo) -> Result<PeerConnection, Error> {
        println!("Connecting to {}:{}", peer.ip, peer.port);
        let stream = try!(TcpStream::connect((peer.ip, peer.port)));
        let mut conn = PeerConnection {
            stream: stream,
            have:   vec![false; metainfo.info.pieces.len()],
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
                }
            },
            Message::Have(have_index) => {
                self.have[have_index] = true;
            }
            _ => panic!("Need to process message: {:?}", message)
        };
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

fn convert_big_endian_to_integer(bytes: &[u8]) -> usize {
    bytes[0] as usize * 16777216 + 
    bytes[1] as usize * 65536 + 
    bytes[2] as usize * 256 + 
    bytes[3] as usize
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
