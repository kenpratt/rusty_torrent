extern crate sha1;

use bencode;
use bencode::{Bencode, FromBencode};
use bencode::util::ByteString;
use std::fs::File;
use std::io::Read;

use decoder;

#[derive(PartialEq, Debug)]
pub struct Metainfo {
    pub announce: String,
    pub info: Info,
    pub info_hash: Vec<u8>,
    pub created_by: String,
}

impl FromBencode for Metainfo {
    type Err = decoder::Error;

    fn from_bencode(bencode: &bencode::Bencode) -> Result<Metainfo, decoder::Error> {
        match bencode {
            &Bencode::Dict(ref m) => {
                let info_bytes = get_field_as_bencoded_bytes!(m, "info");
                let info_hash = calculate_sha1(&info_bytes);

                let metainfo = Metainfo{
                    announce: get_field!(m, "announce"),
                    info: get_field!(m, "info"),
                    info_hash: info_hash,
                    created_by: get_field_with_default!(m, "created by", "".to_string()),
                };
                Ok(metainfo)
            }
            _ => Err(decoder::Error::NotADict)
        }
    }
}

pub type Sha1 = Vec<u8>;

#[derive(PartialEq, Debug)]
pub struct Info {
    pub piece_length: u32,
    pub pieces: Vec<Sha1>,
    pub name: String,
    pub length: u64,
}

impl FromBencode for Info {
    type Err = decoder::Error;

    fn from_bencode(bencode: &bencode::Bencode) -> Result<Info, decoder::Error> {
        match bencode {
            &Bencode::Dict(ref m) => {
                let pieces_bytes: Vec<u8> = get_field_as_bytes!(m, "pieces");
                let pieces: Vec<Vec<u8>> = pieces_bytes.chunks(20).map(|v| v.to_owned()).collect();

                let info = Info {
                    piece_length: get_field!(m, "piece length"),
                    pieces: pieces,
                    name: get_field!(m, "name"),
                    length: get_field!(m, "length"),
                };
                Ok(info)
            }
            _ => Err(decoder::Error::NotADict)
        }
    }
}

pub fn parse(filename: &str) -> Result<Metainfo, decoder::Error> {
    // read the torrent file into a byte vector
    let mut f = try!(File::open(filename));
    let mut v = Vec::new();
    try!(f.read_to_end(&mut v));

    // decode the byte vector into a struct
    let bencode = try!(bencode::from_vec(v));
    let result = try!(FromBencode::from_bencode(&bencode));

    Ok(result)
}

fn calculate_sha1(input: &[u8]) -> Sha1 {
    let mut hasher = sha1::Sha1::new();
    hasher.update(input);
    hasher.digest()
}
