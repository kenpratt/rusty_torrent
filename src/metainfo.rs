extern crate bencode;

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
    created_by: String,
}

impl FromBencode for Metainfo {
    type Err = MetainfoError;

    fn from_bencode(bencode: &bencode::Bencode) -> Result<Metainfo, MetainfoError> {
        match bencode {
            &Bencode::Dict(ref m) => {
                let metainfo = Metainfo{
                    announce: get_field!(m, "announce"),
                    info: get_field!(m, "info"),
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
    name: String,
}

impl FromBencode for Info {
    type Err = MetainfoError;

    fn from_bencode(bencode: &bencode::Bencode) -> Result<Info, MetainfoError> {
        match bencode {
            &Bencode::Dict(ref m) => {
                let info = Info{
                    piece_length: get_field!(m, "piece length"),
                    name: get_field!(m, "name"),
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
