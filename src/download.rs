use std::{convert, io};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpStream};

use metainfo::Metainfo;
use tracker_response::Peer;

pub fn download(info: &Metainfo, peers: &[Peer]) {
    for peer in peers {
        if peer.ip != Ipv4Addr::new(207, 251, 103, 46) {
            match PeerConnection::connect(peer, &info.info_hash) {
                Ok(_) => println!("Peer done"),
                Err(e) => println!("Error: {:?}", e)
            }
        }
    }
}

struct PeerConnection {
    stream: TcpStream,
}

impl PeerConnection {
    fn connect(peer: &Peer, info_hash: &[u8]) -> Result<PeerConnection, Error> {
        println!("Connecting to {}:{}", peer.ip, peer.port);
        let stream = try!(TcpStream::connect((peer.ip, peer.port)));
        let mut conn = PeerConnection { stream: stream };
        try!(conn.handshake(info_hash));
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

    fn read_n(&mut self, bytes_to_read: usize) -> Result<Vec<u8>, Error> {
        let mut buf = vec![];
        let bytes_read = (&mut self.stream).take(bytes_to_read as u64).read_to_end(&mut buf);
        match bytes_read {
            Ok(n) if n == bytes_to_read  => Ok(buf),
            Ok(_)  => Err(Error::NotEnoughData),
            Err(e) => try!(Err(e))
        }
    }
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
