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
use std::{convert, env, io, thread};
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use download::Download;

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

fn run(filename: &str) -> Result<(), Error> {
    let our_peer_id = generate_peer_id();
    println!("Using peer id: {}", our_peer_id);

    // parse .torrent file
    let metainfo = try!(metainfo::parse(filename));

    // connect to tracker and download list of peers
    let peers = try!(tracker::get_peers(&our_peer_id, &metainfo));
    println!("Found {} peers", peers.len());

    // create the download metadata object and stuff it inside a reference-counted mutex
    let download = try!(Download::new(our_peer_id, metainfo));
    let download_mutex = Arc::new(Mutex::new(download));

    // spawn threads to connect to peers and start the download
    let peer_threads: Vec<thread::JoinHandle<()>> = peers.into_iter().map(|peer| {
        let mutex = download_mutex.clone();
        thread::spawn(move || {
            match peer_connection::connect(&peer, mutex) {
                Ok(_) => println!("Peer done"),
                Err(e) => println!("Error: {:?}", e)
            }
        })
    }).collect();

    // wait for peers to complete
    for thr in peer_threads {
        thr.join();
    }

    Ok(())
}

fn generate_peer_id() -> String {
    let mut rng = rand::thread_rng();
    let rand_chars: String = rng.gen_ascii_chars().take(20 - PEER_ID_PREFIX.len()).collect();
    format!("{}{}", PEER_ID_PREFIX, rand_chars)
}

#[derive(Debug)]
pub enum Error {
    DecoderError(decoder::Error),
    DownloadError(download::Error),
    TrackerError(tracker::Error),
}

impl convert::From<decoder::Error> for Error {
    fn from(err: decoder::Error) -> Error {
        Error::DecoderError(err)
    }
}

impl convert::From<download::Error> for Error {
    fn from(err: download::Error) -> Error {
        Error::DownloadError(err)
    }
}

impl convert::From<tracker::Error> for Error {
    fn from(err: tracker::Error) -> Error {
        Error::TrackerError(err)
    }
}