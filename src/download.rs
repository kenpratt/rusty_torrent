use std::{convert, io};
use std::fs::File;
use std::io::{Seek, Write};
use std::path::Path;
use std::sync::mpsc::{Sender, SendError};

use hash::{calculate_sha1, Sha1};
use ipc::IPC;
use metainfo::Metainfo;
use rand;
use rand::Rng;

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

        // create pieces
        let mut pieces = vec![];
        for i in 0..num_pieces {
            let len = if i < (num_pieces - 1) {
                piece_length
            } else {
                (file_length - (piece_length as u64 * (num_pieces as u64 - 1))) as u32
            };
            pieces.push(Piece::new(i, len, piece_length, metainfo.info.pieces[i as usize].clone()));
        }

        // create file
        let path = Path::new("downloads").join(&metainfo.info.name);
        let file = try!(File::create(path));
        try!(file.set_len(metainfo.info.length));

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
            try!(piece.store(&mut self.file, block_index, data));
        }

        // notify peers that this block is complete
        self.broadcast(IPC::BlockComplete(piece_index, block_index));

        // notify peers if download is complete
        if self.is_complete() {
            self.broadcast(IPC::DownloadComplete);
        }

        Ok(())
    }

    pub fn next_block_to_request(&self, peer_has_pieces: &[bool]) -> Option<(u32, u32, u32)> {
        match self.get_random_incomplete_piece(peer_has_pieces) {
            Some(piece) => match piece.get_random_incomplete_to_request() {
                Some(block) => Some((piece.index, block.index, block.length)),
                None => None
            },
            None => None
        }
    }

    fn is_complete(&self) -> bool {
        for piece in self.pieces.iter() {
            if !piece.is_complete {
                return false
            }
        }
        true
    }

    fn get_random_incomplete_piece(&self, peer_has_pieces: &[bool]) -> Option<&Piece> {
        let incomplete_pieces: Vec<&Piece> = self.pieces.iter().filter(|x| !x.is_complete && peer_has_pieces[x.index as usize]).collect();
        rand::thread_rng().choose(&incomplete_pieces).map(|x| *x)
    }

    fn broadcast(&self, ipc: IPC) {
        for channel in self.peer_channels.iter() {
            channel.send(ipc.clone());
        }
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
    fn new(index: u32, length: u32, piece_length: u32, hash: Sha1) -> Piece {
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

        Piece {
            index:        index,
            piece_length: piece_length,
            hash:         hash,
            blocks:       blocks,
            is_complete:  false
        }
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

    fn get_random_incomplete_to_request(&self) -> Option<&Block> {
        if self.is_complete {
            return None
        }

        let empty_blocks: Vec<&Block> = self.blocks.iter().filter(|x| x.data.is_none()).collect();
        rand::thread_rng().choose(&empty_blocks).map(|x| *x)
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
    IoError(io::Error),
    SendError(SendError<IPC>),
}

impl convert::From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IoError(err)
    }
}

impl convert::From<SendError<IPC>> for Error {
    fn from(err: SendError<IPC>) -> Error {
        Error::SendError(err)
    }
}
