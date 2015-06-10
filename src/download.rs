use std::net::{TcpStream,Ipv4Addr};
use metainfo::Metainfo;
use tracker_response::Peer;
use std::io::{Write, Read};

pub fn download(info: &Metainfo, peers: &[Peer]) {
    for peer in peers {
        if peer.ip != Ipv4Addr::new(207, 251, 103, 46) {
            PeerConnection::connect(peer, &info.info_hash);
        }
    }
}

struct PeerConnection {
    stream: TcpStream,
}

impl PeerConnection {
    fn connect(peer: &Peer, info_hash: &[u8]) -> Option<PeerConnection> {
        println!("Connecting to {}:{}", peer.ip, peer.port);
        match TcpStream::connect((peer.ip, peer.port)) {
            Ok(stream) => {
                let mut conn = PeerConnection { stream: stream };
                conn.handshake(info_hash);
                Some(conn)
            },
            Err(_) => {
                println!("Failed to connect to {}:{}", peer.ip, peer.port);
                None
            }
        }
    }

    fn handshake(&mut self, info_hash: &[u8]) {
        self.send_handshake(info_hash);
        self.receive_handshake();
    }

    fn send_handshake(&mut self, info_hash: &[u8]) {
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
        self.stream.write_all(&message).unwrap();
    }

    fn receive_handshake(&mut self) {
        let pstrlen = self.read_n(1);
        let pstr = self.read_n(pstrlen[0] as usize);
        let reserved = self.read_n(8);
        let info_hash = self.read_n(20);
        let peer_id = self.read_n(20);
    }

    fn read_n(&mut self, bytes_to_read: usize) -> Vec<u8> {
        let mut buf = vec![];
        let bytes_read = (&mut self.stream).take(bytes_to_read as u64).read_to_end(&mut buf);
        match bytes_read {
            Ok(n)  => assert_eq!(bytes_to_read as usize, n),
            Err(_) => panic!("Didn't read enough"),
        }
        buf
    }
}
