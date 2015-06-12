extern crate bencode;

mod decoder;
mod download;
mod hash;
mod metainfo;
mod tracker;
mod tracker_response;

use std::env;

fn main() {
    // parse command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: rusty_torrent path/to/myfile.torrent")
    }
    let filename = &args[1];

    match run(filename) {
        Ok(_)  => println!("Yay, it worked!"),
        Err(e) => println!("Oops, it failed: {:?}", e)
    }
}

fn run(filename: &str) -> Result<(), tracker::Error> {
    let metainfo = try!(metainfo::parse(filename));
    let peers = try!(tracker::get_peers(&metainfo));
    download::download(&metainfo, &peers);
    Ok(())
}
