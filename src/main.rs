extern crate bencode;
extern crate getopts;
extern crate rand;

mod decoder;
mod download;
mod hash;
mod ipc;
mod listener;
mod metainfo;
mod peer_connection;
mod request_metadata;
mod request_queue;
mod tracker;
mod tracker_response;

use getopts::Options;
use rand::Rng;
use std::{any, convert, env, process, thread};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use download::Download;

const PEER_ID_PREFIX: &'static str = "-RC0001-";

fn main() {
    // parse command-line arguments & options
    let args: Vec<String> = env::args().collect();
    let program = &args[0];
    let mut opts = Options::new();
    opts.optopt("p", "port", "set listen port to", "6881");
    opts.optflag("h", "help", "print this help menu");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => { panic!(f.to_string()) }
    };

    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }

    let port = match matches.opt_str("p") {
        Some(port_string) => {
            let port: Result<u16,_> = port_string.parse();
            match port {
                Ok(p) => p,
                Err(_) => return abort(&program, opts, format!("Bad port number: {}", port_string))
            }
        },
        None => 6881
    };

    let rest = matches.free;
    if rest.len() != 1 {
        abort(&program, opts, format!("You must provide exactly 1 argument to rusty_torrent: {:?}", rest))
    }

    let filename = &rest[0];
    match run(filename, port) {
        Ok(_) => {},
        Err(e) => println!("Error: {:?}", e)
    }
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options] path/to/myfile.torrent", program);
    print!("{}", opts.usage(&brief));
}

fn abort(program: &str, opts: Options, err: String) {
    println!("{}", err);
    print_usage(program, opts);
    process::exit(1);
}

fn run(filename: &str, listener_port: u16) -> Result<(), Error> {
    let our_peer_id = generate_peer_id();
    println!("Using peer id: {}", our_peer_id);

    // parse .torrent file
    let metainfo = try!(metainfo::parse(filename));

    // connect to tracker and download list of peers
    let peers = try!(tracker::get_peers(&our_peer_id, &metainfo, listener_port));
    println!("Found {} peers", peers.len());

    // create the download metadata object and stuff it inside a reference-counted mutex
    let download = try!(Download::new(our_peer_id, metainfo));
    let download_mutex = Arc::new(Mutex::new(download));

    // spawn thread to listen for incoming request
    listener::start(listener_port, download_mutex.clone());

    // spawn threads to connect to peers and start the download
    let peer_threads: Vec<JoinHandle<()>> = peers.into_iter().map(|peer| {
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
        try!(thr.join());
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
    Any(Box<any::Any + Send>),
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

impl convert::From<Box<any::Any + Send>> for Error {
    fn from(err: Box<any::Any + Send>) -> Error {
        Error::Any(err)
    }
}
