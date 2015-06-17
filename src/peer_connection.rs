use std::{any, convert, fmt, io};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, RecvError, Sender, SendError};
use std::thread;

use download;
use download::{BLOCK_SIZE, Download};
use ipc::IPC;
use tracker_response::Peer;

const PROTOCOL: &'static str = "BitTorrent protocol";
const MAX_CONCURRENT_REQUESTS: u32 = 10;

pub fn connect(peer: &Peer, download_mutex: Arc<Mutex<Download>>) -> Result<(), Error> {
    PeerConnection::connect(peer, download_mutex)
}

pub fn accept(stream: TcpStream, download_mutex: Arc<Mutex<Download>>) -> Result<(), Error> {
    PeerConnection::accept(stream, download_mutex)
}

pub struct PeerConnection {
    halt: bool,
    download_mutex: Arc<Mutex<Download>>,
    stream: TcpStream,
    me: PeerMetadata,
    them: PeerMetadata,
    requests_in_progress: Vec<RequestMetadata>,
    rx: Receiver<IPC>,
    tx: Sender<IPC>,
}

impl PeerConnection {
    fn connect(peer: &Peer, download_mutex: Arc<Mutex<Download>>) -> Result<(), Error> {
        println!("Connecting to {}:{}", peer.ip, peer.port);
        let stream = try!(TcpStream::connect((peer.ip, peer.port)));
        PeerConnection::new(stream, download_mutex, true)
    }

    fn accept(stream: TcpStream, download_mutex: Arc<Mutex<Download>>) -> Result<(), Error> {
        println!("Received connection from a peer!");
        PeerConnection::new(stream, download_mutex, false)
    }

    fn new(stream: TcpStream, download_mutex: Arc<Mutex<Download>>, send_handshake_first: bool) -> Result<(), Error> {
        let have_pieces = {
            let download = download_mutex.lock().unwrap();
            download.have_pieces()
        };
        let num_pieces = have_pieces.len();

        // register IPC channel with Download
        let (tx, rx) = channel::<IPC>();
        {
            let mut download = download_mutex.lock().unwrap();
            download.register_peer(tx.clone());
        }

        let conn = PeerConnection {
            halt: false,
            download_mutex: download_mutex,
            stream: stream,
            me: PeerMetadata::new(have_pieces),
            them: PeerMetadata::new(vec![false; num_pieces]),
            requests_in_progress: vec![],
            rx: rx,
            tx: tx,
        };

        try!(conn.run(send_handshake_first));

        println!("Disconnected");
        Ok(())
    }

    fn run(mut self, send_handshake_first: bool) -> Result<(), Error> {
        if send_handshake_first {
            try!(self.send_handshake());
            try!(self.receive_handshake());
        } else {
            try!(self.receive_handshake());
            try!(self.send_handshake());
        }

        println!("Handshake complete");

        // spawn a thread to funnel incoming messages from the socket into the channel
        let tx_clone = self.tx.clone();
        let stream_clone = self.stream.try_clone().unwrap();
        let funnel_thread = thread::spawn(move || MessageFunnel::start(stream_clone, tx_clone));

        // send a bitfield message letting peer know what we have
        try!(self.send_bitfield());

        // process messages received on the channel (both from the remote peer, and from Downlad)
        while !self.halt {
            let message = try!(self.rx.recv());
            try!(self.process(message));
        }

        println!("Disconnecting");
        try!(self.stream.shutdown(Shutdown::Both));
        try!(funnel_thread.join());
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
        try!(read_n(&mut self.stream, pstrlen[0] as u32)); // ignore pstr
        try!(read_n(&mut self.stream, 8)); // ignore reserved
        let info_hash = try!(read_n(&mut self.stream, 20));
        let peer_id = try!(read_n(&mut self.stream, 20));

        {
            let download = self.download_mutex.lock().unwrap();

            // validate info hash
            if info_hash != download.metainfo.info_hash {
                return Err(Error::InvalidInfoHash);
            }

            // validate peer id
            let our_peer_id: Vec<u8> = download.our_peer_id.bytes().collect();
            if peer_id == our_peer_id {
                return Err(Error::ConnectingToSelf);
            }
        }

        Ok(())
    }

    fn send_message(&mut self, message: Message) -> Result<(), Error> {
        println!("Sending: {:?}", message);
        try!(self.stream.write_all(&message.serialize()));
        Ok(())
    }

    fn process(&mut self, ipc: IPC) -> Result<(), Error> {
        match ipc {
            IPC::Message(message) => self.process_message(message),
            IPC::BlockComplete(piece_index, block_index) => {
                match self.requests_in_progress.iter().position(|r| r.matches(piece_index, block_index)) {
                    Some(i) => {
                        let r = self.requests_in_progress.remove(i);
                        self.send_message(Message::Cancel(r.piece_index, r.offset, r.block_length))
                    },
                    None => Ok(())
                }
            },
            IPC::PieceComplete(piece_index) => {
                self.me.has_pieces[piece_index as usize] = true;
                try!(self.update_my_interested_status());
                try!(self.send_message(Message::Have(piece_index)));
                Ok(())
            },
            IPC::DownloadComplete => {
                self.halt = true;
                try!(self.update_my_interested_status());
                Ok(())
            },
        }
    }

    fn process_message(&mut self, message: Message) -> Result<(), Error> {
        println!("Received: {:?}", message);
        match message {
            Message::KeepAlive => {},
            Message::Choke => {
                self.me.is_choked = true;
            },
            Message::Unchoke => {
                if self.me.is_choked {
                    self.me.is_choked = false;
                    try!(self.request_more_blocks());
                }
            },
            Message::Interested => {
                self.them.is_interested = true;
                try!(self.unchoke_them());
            },
            Message::NotInterested => {
                self.them.is_interested = false;
                // TODO stop sending them blocks
            },
            Message::Have(have_index) => {
                self.them.has_pieces[have_index as usize] = true;
                try!(self.update_my_interested_status());
                try!(self.request_more_blocks());
            },
            Message::Bitfield(bytes) => {
                for have_index in 0..self.them.has_pieces.len() {
                    let bytes_index = have_index / 8;
                    let index_into_byte = have_index % 8;
                    let byte = bytes[bytes_index];
                    let mask = 1 << (7 - index_into_byte);
                    let value = (byte & mask) != 0;
                    self.them.has_pieces[have_index] = value;
                };
                try!(self.update_my_interested_status());
                try!(self.request_more_blocks());
            },
            Message::Request(piece_index, offset, length) => {
                // TODO implement Request
            },
            Message::Piece(piece_index, offset, data) => {
                let block_index = offset / BLOCK_SIZE;
                self.requests_in_progress.retain(|r| !r.matches(piece_index, block_index));
                {
                    let mut download = self.download_mutex.lock().unwrap();
                    try!(download.store(piece_index, block_index, data))
                }
                try!(self.update_my_interested_status());
                try!(self.request_more_blocks());
            },
            Message::Cancel(piece_index, offset, length) => {
                // TODO implement Cancel
            },
            _ => return Err(Error::UnknownRequestType(message))
        };
        Ok(())
    }

    fn update_my_interested_status(&mut self) -> Result<(), Error> {
        let am_interested = {
            let download = self.download_mutex.lock().unwrap();
            download.is_interested(&self.them.has_pieces)
        };

        if self.me.is_interested != am_interested {
            self.me.is_interested = am_interested;
            let message = if am_interested { Message::Interested } else { Message::NotInterested };
            self.send_message(message)
        } else {
            Ok(())
        }
    }

    fn send_bitfield(&mut self) -> Result<(), Error> {
        let mut bytes: Vec<u8> = vec![0; (self.me.has_pieces.len() as f64 / 8 as f64).ceil() as usize];
        for have_index in 0..self.me.has_pieces.len() {
            let bytes_index = have_index / 8;
            let index_into_byte = have_index % 8;
            if self.me.has_pieces[have_index] {
                let mask = 1 << (7 - index_into_byte);
                bytes[bytes_index] |= mask;
            }
        };
        self.send_message(Message::Bitfield(bytes))
    }

    fn request_more_blocks(&mut self) -> Result<(), Error> {
        if self.me.is_choked || !self.me.is_interested {
            return Ok(())
        }

        while self.requests_in_progress.len() < MAX_CONCURRENT_REQUESTS as usize {
            let next_block_to_request = {
                let download = self.download_mutex.lock().unwrap();
                download.next_block_to_request(&self.them.has_pieces)
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

    fn unchoke_them(&mut self) -> Result<(), Error> {
        if self.them.is_choked {
            self.them.is_choked = false;
            try!(self.send_message(Message::Unchoke));
            // TODO start sending them blocks
        }
        Ok(())
    }
}

struct PeerMetadata {
    has_pieces: Vec<bool>,
    is_choked: bool,
    is_interested: bool,
}

impl PeerMetadata {
    fn new(has_pieces: Vec<bool>) -> PeerMetadata {
        PeerMetadata {
            has_pieces: has_pieces,
            is_choked: true,
            is_interested: false,
        }
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

#[derive(Clone)]
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
    Port, // TODO Add params
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
    InvalidInfoHash,
    ConnectingToSelf,
    DownloadError(download::Error),
    IoError(io::Error),
    NotEnoughData(u32, u32),
    UnknownRequestType(Message),
    ReceiveError(RecvError),
    SendError(SendError<IPC>),
    Any(Box<any::Any + Send>),
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

impl convert::From<Box<any::Any + Send>> for Error {
    fn from(err: Box<any::Any + Send>) -> Error {
        Error::Any(err)
    }
}
