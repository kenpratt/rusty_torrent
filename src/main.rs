extern crate bencode;

mod decoder;
mod download;
mod hash;
mod metainfo;
mod peer_connection;
mod tracker;
mod tracker_response;

use std::env;
use std::net::Ipv4Addr;

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
    let metainfo = try!(metainfo::parse(filename));
    let peers = try!(tracker::get_peers(&metainfo));
    for peer in peers {
        if peer.ip != Ipv4Addr::new(207, 251, 103, 46) {
            match peer_connection::connect(&peer, &metainfo) {
                Ok(_) => println!("Peer done"),
                Err(e) => println!("Error: {:?}", e)
            }
        }
    }
    Ok(())
}
