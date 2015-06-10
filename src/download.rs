use std::net::{TcpStream,Ipv4Addr};
use metainfo::Metainfo;
use tracker_response::Peer;
use std::io::{Write, Read, Bytes};

pub fn download(info: &Metainfo, peers: &[Peer]){
	for peer in peers{
		if peer.ip != Ipv4Addr::new(207,251 ,103 ,46){
			println!("Trying with {} at port {}",peer.ip, peer.port);
			match TcpStream::connect((peer.ip, peer.port)){

				Ok(mut conn) => {
					handshake(&mut conn, &info.info_hash);
				}
				Err(_) => println!("It failed!")
			}
		}
	}
}

fn handshake(conn: &mut TcpStream, info_hash: &[u8]){
	send_handshake(conn, info_hash);
	receive_handshake(conn);
}

fn send_handshake(conn: &mut TcpStream, info_hash: &[u8]){
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
	conn.write_all(&message).unwrap();
}

fn receive_handshake(conn: &mut TcpStream){
 	let inbound_bytes = &mut conn.bytes();
 	let pstrlen = read_bytes(inbound_bytes, 1);
 	let pstr = read_bytes(inbound_bytes, pstrlen[0]);
 	let reserved = read_bytes(inbound_bytes, 8);
 	let info_hash = read_bytes(inbound_bytes, 20);
 	let peer_id = read_bytes(inbound_bytes, 20);
}

fn read_bytes(byte_iter: &mut Bytes<&mut TcpStream>, n: u8) -> Vec<u8> {
	let mut out = vec![];
	for _ in 0..n  {
		let byte = byte_iter.next().unwrap().unwrap();
		out.push(byte);
	}
	out
}