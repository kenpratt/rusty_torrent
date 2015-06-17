use hash::{calculate_sha1, Sha1};
use std::{convert, io};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::sync::mpsc::{Sender, SendError};

use ipc::IPC;
use metainfo::Metainfo;
use request_metadata::RequestMetadata;
use request_queue::RequestQueue;

pub const BLOCK_SIZE: u32 = 16384;

pub struct Download {
    pub our_peer_id: String,
    pub metainfo:    Metainfo,
    pieces:          Vec<Piece>,
    file:            File,
    peer_channels:   Vec<Sender<IPC>>,
}

impl Download {
    pub fn new(our_peer_id: String, metainfo: Metainfo) -> Result<Download, Error> {
        let file_length = metainfo.info.length;
        let piece_length = metainfo.info.piece_length;
        let num_pieces = metainfo.info.num_pieces;

        // create/open file
        let path = Path::new("downloads").join(&metainfo.info.name);
        let mut file = try!(OpenOptions::new().create(true).read(true).write(true).open(path));

        // create pieces
        let mut pieces = vec![];
        for i in 0..num_pieces {
            let len = if i < (num_pieces - 1) {
                piece_length
            } else {
                (file_length - (piece_length as u64 * (num_pieces as u64 - 1))) as u32
            };
            let piece = try!(Piece::new(i, len, piece_length, metainfo.info.pieces[i as usize].clone(), &mut file));
            pieces.push(piece);
        }

        Ok(Download {
            our_peer_id:   our_peer_id,
            metainfo:      metainfo,
            pieces:        pieces,
            file:          file,
            peer_channels: vec![],
        })
    }

    pub fn register_peer(&mut self, channel: Sender<IPC>) {
        self.peer_channels.push(channel);
    }

    pub fn store(&mut self, piece_index: u32, block_index: u32, data: Vec<u8>) -> Result<(), Error> {
        {
            let piece = &mut self.pieces[piece_index as usize];
            if piece.is_complete || piece.has_block(block_index) {
                // if we already have this block, do an early return to avoid re-writing the piece, sending complete messages, etc
                return Ok(())
            }
            try!(piece.store(&mut self.file, block_index, data));
        }

        // notify peers that this block is complete
        self.broadcast(IPC::BlockComplete(piece_index, block_index));

        // notify peers if piece is complete
        if self.pieces[piece_index as usize].is_complete {
            self.broadcast(IPC::PieceComplete(piece_index));
        }

        // notify peers if download is complete
        if self.is_complete() {
            println!("Download complete");
            self.broadcast(IPC::DownloadComplete);
        }

        Ok(())
    }

    pub fn retrive_data(&mut self, request: &RequestMetadata) -> Result<Vec<u8>, Error> {
        let ref piece = self.pieces[request.piece_index as usize];
        if piece.is_complete {
            let offset = (piece.index as u64 * piece.piece_length as u64) + request.offset as u64;
            let file = &mut self.file;
            try!(file.seek(io::SeekFrom::Start(offset)));
            let mut buf = vec![];
            try!(file.take(request.block_length as u64).read_to_end(&mut buf));
            Ok(buf)
        } else {
            Err(Error::MissingPieceData)
        }

    }

    pub fn is_interested(&self, peer_has_pieces: &[bool]) -> bool {
        for piece in self.pieces.iter() {
            if !piece.is_complete && peer_has_pieces[piece.index as usize] {
                return true;
            }
        }
        false
    }

    pub fn incomplete_blocks_of_interest(&self, peer_has_pieces: &[bool], request_queue: &RequestQueue) -> Vec<(u32, u32, u32)> {
        let mut out = vec![];
        for piece in self.pieces.iter() {
            if !piece.is_complete && peer_has_pieces[piece.index as usize] {
                for block in piece.blocks.iter() {
                    if block.data.is_none() && !request_queue.has(piece.index, block.index) {
                        out.push((piece.index, block.index, block.length));
                    }
                }
            }
        }
        out
    }

    pub fn have_pieces(&self) -> Vec<bool> {
        self.pieces.iter().map(|p| p.is_complete).collect()
    }

    fn is_complete(&self) -> bool {
        for piece in self.pieces.iter() {
            if !piece.is_complete {
                return false
            }
        }
        true
    }

    fn broadcast(&mut self, ipc: IPC) {
        self.peer_channels.retain(|channel| {
            match channel.send(ipc.clone()) {
                Ok(_) => true,
                Err(SendError(_)) => false // presumably channel has disconnected
            }
        });
    }
}

struct Piece {
    index:        u32,
    piece_length: u32,
    hash:         Sha1,
    blocks:       Vec<Block>,
    is_complete:  bool,
}

impl Piece {
    fn new(index: u32, length: u32, piece_length: u32, hash: Sha1, file: &mut File) -> Result<Piece, Error> {
        // create blocks
        let mut blocks = vec![];
        let num_blocks = (length as f64 / BLOCK_SIZE as f64).ceil() as u32;
        for i in 0..num_blocks {
            let len = if i < (num_blocks - 1) {
                BLOCK_SIZE
            } else {
                length - (BLOCK_SIZE * (num_blocks - 1))
            };
            blocks.push(Block::new(i, len));
        }

        // check if this piece is already complete
        let offset = index as u64 * piece_length as u64;
        try!(file.seek(io::SeekFrom::Start(offset)));
        let mut buf = vec![];
        try!(file.take(length as u64).read_to_end(&mut buf));
        let is_complete = hash == calculate_sha1(&buf);

        Ok(Piece {
            index:        index,
            piece_length: piece_length,
            hash:         hash,
            blocks:       blocks,
            is_complete:  is_complete,
        })
    }

    fn store(&mut self, file: &mut File, block_index: u32, data: Vec<u8>) -> Result<(), Error> {
        {
            // store data in the appropriate block
            let block = &mut self.blocks[block_index as usize];
            block.data = Some(data);
        }

        if self.have_all_blocks() {
            // concatenate data from blocks together
            let mut data = vec![];
            for block in self.blocks.iter() {
                data.extend(block.data.clone().unwrap());
            }

            // validate that piece data matches SHA1 hash
            println!("Have {} bytes of data for index {}", data.len(), self.index);
            if self.hash == calculate_sha1(&data) {
                println!("Piece {} is complete and correct, writing to the file.", self.index);
                let offset = self.index as u64 * self.piece_length as u64;
                try!(file.seek(io::SeekFrom::Start(offset)));
                try!(file.write_all(&data));
                self.clear_block_data();
                self.is_complete = true;
            } else {
                println!("Piece is corrupt, deleting downloaded piece data!");
                self.clear_block_data();
            }
        }
        Ok(())
    }

    fn has_block(&self, block_index: u32) -> bool {
        self.blocks[block_index as usize].data.is_some()
    }

    fn have_all_blocks(&self) -> bool {
        for block in self.blocks.iter() {
            if block.data.is_none() {
                return false
            }
        }
        true
    }

    fn clear_block_data(&mut self) {
        for block in self.blocks.iter_mut() {
            block.data = None;
        }
    }
}

struct Block {
    index:  u32,
    length: u32,
    data:   Option<Vec<u8>>,
}

impl Block {
    fn new(index: u32, length: u32) -> Block {
        Block {
            index:  index,
            length: length,
            data:   None,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    MissingPieceData,
    IoError(io::Error),
}

impl convert::From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}
