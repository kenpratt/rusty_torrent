use std::{convert, io};
use std::fmt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, RecvError, Sender, SendError};
use std::thread;

use download;
use download::{BLOCK_SIZE, Download};
use ipc::IPC;
use tracker_response::Peer;

const PROTOCOL: &'static str = "BitTorrent protocol";
const MAX_CONCURRENT_REQUESTS: u32 = 10;

pub fn connect(peer: &Peer, download: Arc<Mutex<Download>>) -> Result<PeerConnection, Error> {
    PeerConnection::connect(peer, download)
}

pub struct PeerConnection {
    download_mutex: Arc<Mutex<Download>>,
    stream: TcpStream,
    have: Vec<bool>,
    am_i_choked: bool,
    am_i_interested: bool,
    are_they_choked: bool,
    are_they_interested: bool,
    requests_in_progress: Vec<RequestMetadata>,
    rx: Receiver<IPC>,
    tx: Sender<IPC>,
}

impl PeerConnection {
    fn connect(peer: &Peer, download_mutex: Arc<Mutex<Download>>) -> Result<PeerConnection, Error> {
        println!("Connecting to {}:{}", peer.ip, peer.port);
        let stream = try!(TcpStream::connect((peer.ip, peer.port)));
        let num_pieces = {
            let download = download_mutex.lock().unwrap();
            download.metainfo.info.num_pieces
        };

        let (tx, rx) = channel::<IPC>();
        {
            let mut download = download_mutex.lock().unwrap();
            download.register_peer(tx.clone());
        }

        let mut conn = PeerConnection {
            download_mutex: download_mutex,
            stream: stream,
            have: vec![false; num_pieces as usize],
            am_i_choked: true,
            am_i_interested: false,
            are_they_choked: true,
            are_they_interested: false,
            requests_in_progress: vec![],
            rx: rx,
            tx: tx,
        };
        try!(conn.run());
        println!("Disconnecting from {}:{}", peer.ip, peer.port);
        Ok(conn)
    }

    fn run(&mut self) -> Result<(), Error> {
        try!(self.send_handshake());
        try!(self.receive_handshake());

        // spawn a thread to funnel incoming messages from the socket into the channel
        let tx_clone = self.tx.clone();
        let stream_clone = self.stream.try_clone().unwrap();
        thread::spawn(move || MessageFunnel::start(stream_clone, tx_clone));

        // process messages received on the channel (both from the remote peer, and from Downlad)
        let mut is_complete = false;
        while !is_complete {
            let message = try!(self.rx.recv());
            is_complete = try!(self.process(message));
        }
        println!("Download complete, disconnecting");
        Ok(())
    }

    fn send_handshake(&mut self) -> Result<(), Error> {
        let message = {
            let download = self.download_mutex.lock().unwrap();
            let mut message = vec![];
            message.push(PROTOCOL.len() as u8);
            message.extend(PROTOCOL.bytes());
            message.extend(vec![0; 8].into_iter());
            message.extend(download.metainfo.info_hash.iter().cloned());
            message.extend(download.our_peer_id.bytes());
            message
        };
        try!(self.stream.write_all(&message));
        Ok(())
    }

    fn receive_handshake(&mut self) -> Result<(), Error> {
        let pstrlen = try!(read_n(&mut self.stream, 1));
        let pstr = try!(read_n(&mut self.stream, pstrlen[0] as u32));
        let reserved = try!(read_n(&mut self.stream, 8));
        let info_hash = try!(read_n(&mut self.stream, 20));
        let peer_id = try!(read_n(&mut self.stream, 20));
        Ok(())
    }

    fn send_message(&mut self, message: Message) -> Result<(), Error> {
        println!("Sending: {:?}", message);
        try!(self.stream.write_all(&message.serialize()));
        Ok(())
    }

    fn process(&mut self, ipc: IPC) -> Result<bool, Error> {
        match ipc {
            IPC::Message(message) => self.process_message(message),
            IPC::CancelRequest(piece_index, block_index) => {
                match self.requests_in_progress.iter().position(|r| r.matches(piece_index, block_index)) {
                    Some(i) => {
                        let r = self.requests_in_progress.remove(i);
                        try!(self.send_message(Message::Cancel(r.piece_index, r.offset, r.block_length)));
                    },
                    None => {}
                }
                Ok(false)
            }
        }
    }

    fn process_message(&mut self, message: Message) -> Result<bool, Error> {
        match message {
            Message::KeepAlive => {},
            Message::Bitfield(bytes) => {
                for have_index in 0..self.have.len() {
                    let bytes_index = have_index / 8;
                    let index_into_byte = have_index % 8;
                    let byte = bytes[bytes_index];
                    let value = (byte & (1 << (7 - index_into_byte))) != 0;
                    self.have[have_index] = value;
                };
                try!(self.send_interested());
            },
            Message::Have(have_index) => {
                self.have[have_index as usize] = true;
                try!(self.send_interested());
            },
            Message::Unchoke => {
                if self.am_i_choked {
                    self.am_i_choked = false;
                    try!(self.request_more_blocks());
                }
            }
            Message::Piece(piece_index, offset, data) => {
                let block_index = offset / BLOCK_SIZE;
                self.requests_in_progress.retain(|r| !r.matches(piece_index, block_index));

                let is_complete = {
                    let mut download = self.download_mutex.lock().unwrap();
                    try!(download.store(piece_index, block_index, data))
                };
                if is_complete {
                    return Ok(true)
                } else {
                    try!(self.request_more_blocks());
                }
            }
            Message::Choke => {
                self.am_i_choked = true;
            }
            _ => return Err(Error::UnknownRequestType(message))
        };
        Ok(false)
    }

    fn send_interested(&mut self) -> Result<(), Error> {
        if self.am_i_interested == false {
            self.am_i_interested = true;
            try!(self.send_message(Message::Interested));
        }
        Ok(())
    }

    fn request_more_blocks(&mut self) -> Result<(), Error> {
        if self.am_i_choked == true {
            return Ok(())
        }
        while self.requests_in_progress.len() < MAX_CONCURRENT_REQUESTS as usize {
            let next_block_to_request = {
                let download = self.download_mutex.lock().unwrap();
                download.next_block_to_request(&self.have)
            };
            match next_block_to_request {
                Some((piece_index, block_index, block_length)) => {
                    let offset = block_index * BLOCK_SIZE;
                    try!(self.send_message(Message::Request(piece_index, offset, block_length)));
                    self.requests_in_progress.push(RequestMetadata {
                        piece_index: piece_index,
                        block_index: block_index,
                        offset: offset,
                        block_length: block_length,
                    });
                },
                None => {
                    println!("We've downloaded all the pieces we can from this peer.");
                    return Ok(())
                }
            }
        }
        Ok(())
    }
}

struct MessageFunnel {
    stream: TcpStream,
    tx: Sender<IPC>,
}

impl MessageFunnel {
    fn start(stream: TcpStream, tx: Sender<IPC>) {
        let mut funnel = MessageFunnel {
            stream: stream,
            tx: tx,
        };
        match funnel.run() {
            Ok(_) => {},
            Err(e) => println!("Error: {:?}", e)
        }
    }

    fn run(&mut self) -> Result<(), Error> {
        loop {
            let message = try!(self.receive_message());
            try!(self.tx.send(IPC::Message(message)));
        }
    }

    fn receive_message(&mut self) -> Result<Message, Error> {
        let message_size = bytes_to_u32(&try!(read_n(&mut self.stream, 4)));
        if message_size > 0 {
            let message = try!(read_n(&mut self.stream, message_size));
            Ok(Message::new(&message[0], &message[1..]))
        } else {
            Ok(Message::KeepAlive)
        }
    }
}

fn read_n(stream: &mut TcpStream, bytes_to_read: u32) -> Result<Vec<u8>, Error> {
    let mut buf = vec![];
    let bytes_read = stream.take(bytes_to_read as u64).read_to_end(&mut buf);
    match bytes_read {
        Ok(n) if n == bytes_to_read as usize => Ok(buf),
        Ok(n) => Err(Error::NotEnoughData(bytes_to_read, n as u32)),
        Err(e) => try!(Err(e))
    }
}

struct RequestMetadata {
    piece_index: u32,
    block_index: u32,
    offset: u32,
    block_length: u32,
}

impl RequestMetadata {
    fn matches(&self, piece_index: u32, block_index: u32) -> bool {
        self.piece_index == piece_index && self.block_index == block_index
    }
}

pub enum Message {
    KeepAlive,
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have(u32),
    Bitfield(Vec<u8>),
    Request(u32, u32, u32),
    Piece(u32, u32, Vec<u8>),
    Cancel(u32, u32, u32),
    Port,    // TODO Add params
}

impl Message {
    fn new(id: &u8, body: &[u8]) -> Message {
        match *id {
            0 => Message::Choke,
            1 => Message::Unchoke,
            2 => Message::Interested,
            3 => Message::NotInterested,
            4 => Message::Have(bytes_to_u32(body)),
            5 => Message::Bitfield(body.to_owned()),
            6 => {
                let index = bytes_to_u32(&body[0..4]);
                let offset = bytes_to_u32(&body[4..8]);
                let length = bytes_to_u32(&body[8..12]);
                Message::Request(index, offset, length)
            },
            7 => {
                let index = bytes_to_u32(&body[0..4]);
                let offset = bytes_to_u32(&body[4..8]);
                let data = body[8..].to_owned();
                Message::Piece(index, offset, data)
            },
            8 => {
                let index = bytes_to_u32(&body[0..4]);
                let offset = bytes_to_u32(&body[4..8]);
                let length = bytes_to_u32(&body[8..12]);
                Message::Cancel(index, offset, length)
            },
            9 => Message::Port,
            _ => panic!("Bad message id: {}", id)
        }
    }

    fn serialize(self) -> Vec<u8> {
        let mut payload = vec![];
        match self {
            Message::KeepAlive => {},
            Message::Choke => payload.push(0),
            Message::Unchoke => payload.push(1),
            Message::Interested => payload.push(2),
            Message::NotInterested => payload.push(3),
            Message::Have(index) => {
                payload.push(4);
                payload.extend(u32_to_bytes(index).into_iter());
            },
            Message::Bitfield(bytes) => {
                payload.push(5);
                payload.extend(bytes);
            },
            Message::Request(index, offset, length) => {
                payload.push(6);
                payload.extend(u32_to_bytes(index).into_iter());
                payload.extend(u32_to_bytes(offset).into_iter());
                payload.extend(u32_to_bytes(length).into_iter());
            },
            Message::Piece(index, offset, data) => {
                payload.push(6);
                payload.extend(u32_to_bytes(index).into_iter());
                payload.extend(u32_to_bytes(offset).into_iter());
                payload.extend(data);
            },
            Message::Cancel(index, offset, length) => {
                payload.push(8);
                payload.extend(u32_to_bytes(index).into_iter());
                payload.extend(u32_to_bytes(offset).into_iter());
                payload.extend(u32_to_bytes(length).into_iter());
            },
            Message::Port => payload.push(9),
        };

        // prepend size
        let mut size = u32_to_bytes(payload.len() as u32);
        size.extend(payload);
        size
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
             Message::KeepAlive => write!(f, "KeepAlive"),
             Message::Choke => write!(f, "Choke"),
             Message::Unchoke => write!(f, "Unchoke"),
             Message::Interested => write!(f, "Interested"),
             Message::NotInterested => write!(f, "NotInterested"),
             Message::Have(ref index) => write!(f, "Have({})", index),
             Message::Bitfield(ref bytes) => write!(f, "Bitfield({:?})", bytes),
             Message::Request(ref index, ref offset, ref length) => write!(f, "Request({}, {}, {})", index, offset, length),
             Message::Piece(ref index, ref offset, ref data) => write!(f, "Piece({}, {}, size={})", index, offset, data.len()),
             Message::Cancel(ref index, ref offset, ref length) => write!(f, "Cancel({}, {}, {})", index, offset, length),
             Message::Port => write!(f, "Port"),
        }
    }
}

const BYTE_0: u32 = 256 * 256 * 256;
const BYTE_1: u32 = 256 * 256;
const BYTE_2: u32 = 256;
const BYTE_3: u32 = 1;

fn bytes_to_u32(bytes: &[u8]) -> u32 {
    bytes[0] as u32 * BYTE_0 +
    bytes[1] as u32 * BYTE_1 +
    bytes[2] as u32 * BYTE_2 +
    bytes[3] as u32 * BYTE_3
}

fn u32_to_bytes(integer: u32) -> Vec<u8> {
    let mut rest = integer;
    let first = rest / BYTE_0;
    rest -= first * BYTE_0;
    let second = rest / BYTE_1;
    rest -= second * BYTE_1;
    let third = rest / BYTE_2;
    rest -= third * BYTE_2;
    let fourth = rest;
    vec![first as u8, second as u8, third as u8, fourth as u8]
}

#[derive(Debug)]
pub enum Error {
    DownloadError(download::Error),
    IoError(io::Error),
    NotEnoughData(u32, u32),
    UnknownRequestType(Message),
    ReceiveError(RecvError),
    SendError(SendError<IPC>),
}

impl convert::From<download::Error> for Error {
    fn from(err: download::Error) -> Error {
        Error::DownloadError(err)
    }
}

impl convert::From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl convert::From<RecvError> for Error {
    fn from(err: RecvError) -> Error {
        Error::ReceiveError(err)
    }
}

impl convert::From<SendError<IPC>> for Error {
    fn from(err: SendError<IPC>) -> Error {
        Error::SendError(err)
    }
}
