extern crate bencode;
extern crate sha1;

use self::bencode::{FromBencode, Bencode, NumFromBencodeError, StringFromBencodeError};
use self::bencode::util::ByteString;
use std::fs::File;
use std::io::Read;
use std::{convert, io};

macro_rules! get_field_with_default {
    ($m:expr, $field:expr, $default:expr) => (
        match $m.get(&ByteString::from_str($field)) {
            Some(a) => try!(FromBencode::from_bencode(a)),
            None => $default
        };
    )
}

macro_rules! get_field {
    ($m:expr, $field:expr) => (
        get_field_with_default!($m, $field, return Err(MetainfoError::DoesntContain($field)))
    )
}

#[derive(PartialEq, Debug)]
struct Metainfo {
    announce: String,
    info: Info,
    info_hash: Vec<u8>,
    created_by: String,
}

impl FromBencode for Metainfo {
    type Err = MetainfoError;

    fn from_bencode(bencode: &bencode::Bencode) -> Result<Metainfo, MetainfoError> {
        match bencode {
            &Bencode::Dict(ref m) => {
                let info_bencode = match m.get(&ByteString::from_str("info")) {
                    Some(a) => a,
                    None => return Err(MetainfoError::DoesntContain("info"))
                };
                let info_hash = calculate_sha1(&try!(info_bencode.to_bytes()));

                let metainfo = Metainfo{
                    announce: get_field!(m, "announce"),
                    info: get_field!(m, "info"),
                    info_hash: info_hash,
                    created_by: get_field_with_default!(m, "created by", "".to_string()),
                };
                Ok(metainfo)
            }
            _ => Err(MetainfoError::NotADict)
        }
    }
}

#[derive(PartialEq, Debug)]
struct Info {
    piece_length: u32,
    pieces: Vec<u8>,
    name: String,
    length: u32,
}

impl FromBencode for Info {
    type Err = MetainfoError;

    fn from_bencode(bencode: &bencode::Bencode) -> Result<Info, MetainfoError> {
        match bencode {
            &Bencode::Dict(ref m) => {
                let pieces = match m.get(&ByteString::from_str("pieces")) {
                    Some(a) => {
                        match a {
                            &Bencode::ByteString(ref v) => v.clone(),
                            _ => return Err(MetainfoError::NotAByteString)
                        }
                    },
                    None => return Err(MetainfoError::DoesntContain("pieces"))
                };

                let info = Info{
                    piece_length: get_field!(m, "piece length"),
                    pieces: pieces,
                    name: get_field!(m, "name"),
                    length: get_field!(m, "length"),
                };
                Ok(info)
            }
            _ => Err(MetainfoError::NotADict)
        }
    }
}

#[derive(Debug)]
enum MetainfoError {
    IoError(io::Error),
    DecodingError(bencode::streaming::Error),
    NotADict,
    NotAByteString,
    DoesntContain(&'static str),
    NotANumber(NumFromBencodeError),
    NotAString(StringFromBencodeError),
}

impl convert::From<io::Error> for MetainfoError {
    fn from(err: io::Error) -> MetainfoError {
        MetainfoError::IoError(err)
    }
}

impl convert::From<bencode::streaming::Error> for MetainfoError {
    fn from(err: bencode::streaming::Error) -> MetainfoError {
        MetainfoError::DecodingError(err)
    }
}

impl convert::From<NumFromBencodeError> for MetainfoError {
    fn from(err: NumFromBencodeError) -> MetainfoError {
        MetainfoError::NotANumber(err)
    }
}

impl convert::From<StringFromBencodeError> for MetainfoError {
    fn from(err: StringFromBencodeError) -> MetainfoError {
        MetainfoError::NotAString(err)
    }
}

pub fn run() {
    let result = parse_torrent_file("test_data/flagfromserver.torrent");
    match result {
        Ok(s)  => println!("Yay, it worked: {:?}", s),
        Err(e) => println!("Oops, it failed: {:?}", e)
    }
}

fn parse_torrent_file(filename: &str) -> Result<Metainfo, MetainfoError> {
    // read the torrent file into a byte vector
    let mut f = try!(File::open(filename));
    let mut v = Vec::new();
    try!(f.read_to_end(&mut v));

    // decode the byte vector into a struct
    let bencode = try!(bencode::from_vec(v));
    let result = try!(FromBencode::from_bencode(&bencode));

    Ok(result)
}

fn calculate_sha1(input: &[u8]) -> Vec<u8> {
    let mut hasher = sha1::Sha1::new();
    hasher.update(input);
    hasher.digest()
}
