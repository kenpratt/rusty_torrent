use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;

use download::Download;
use peer_connection;

pub fn start(port: u16, download_mutex: Arc<Mutex<Download>>) -> JoinHandle<()> {
    let tcp_listener = TcpListener::bind(("0.0.0.0", port)).unwrap();
    thread::spawn(move || {
        for stream in tcp_listener.incoming() {
            match stream {
                Ok(s) => handle_connection(s, download_mutex.clone()),
                Err(e) => println!("Error: {:?}", e)
            }
        }
    })
}

fn handle_connection(stream: TcpStream, download_mutex: Arc<Mutex<Download>>) {
    thread::spawn(move || {
        match peer_connection::accept(stream, download_mutex) {
            Ok(_) => println!("Peer done"),
            Err(e) => println!("Error: {:?}", e)
        }
    });
}
