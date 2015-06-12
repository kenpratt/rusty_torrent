extern crate bencode;
extern crate rand;

mod decoder;
mod download;
mod hash;
mod metainfo;
mod peer_connection;
mod tracker;
mod tracker_response;

use rand::Rng;
use std::env;
use std::net::Ipv4Addr;

const PEER_ID_PREFIX: &'static str = "-RC0001-";

fn main() {
    // parse command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: rusty_torrent path/to/myfile.torrent")
    }
    let filename = &args[1];

    match run(filename) {
        Ok(_) => {},
        Err(e) => println!("Error: {:?}", e)
    }
}

fn run(filename: &str) -> Result<(), tracker::Error> {
    let peer_id = generate_peer_id();
    println!("Using peer id: {}", peer_id);
    let metainfo = try!(metainfo::parse(filename));
    let peers = try!(tracker::get_peers(&peer_id, &metainfo));
    for peer in peers {
        if peer.ip != Ipv4Addr::new(207, 251, 103, 46) {
            match peer_connection::connect(&peer_id, &peer, &metainfo) {
                Ok(_) => println!("Peer done"),
                Err(e) => println!("Error: {:?}", e)
            }
        }
    }
    Ok(())
}

fn generate_peer_id() -> String {
    let mut rng = rand::thread_rng();
    let rand_chars: String = rng.gen_ascii_chars().take(20 - PEER_ID_PREFIX.len()).collect();
    format!("{}{}", PEER_ID_PREFIX, rand_chars)
}
