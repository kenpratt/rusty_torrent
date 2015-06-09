extern crate bencode;

use self::bencode::{FromBencode, Bencode, StringFromBencodeError};
use self::bencode::util::ByteString;
use std::fs::File;
use std::io::Read;
use std::{convert, io};

#[derive(PartialEq, Debug)]
struct Metainfo {
    announce: String,
    created_by: String,
}

#[derive(Debug)]
enum MetainfoError {
    NotADict,
    DoesntContain(&'static str),
    NotAString(StringFromBencodeError),
}

impl convert::From<StringFromBencodeError> for MetainfoError {
    fn from(err: StringFromBencodeError) -> MetainfoError {
        MetainfoError::NotAString(err)
    }
}

impl FromBencode for Metainfo {
    type Err = MetainfoError;

    fn from_bencode(bencode: &bencode::Bencode) -> Result<Metainfo, MetainfoError> {
        match bencode {
            &Bencode::Dict(ref m) => {
                let announce = match m.get(&ByteString::from_str("announce")) {
                    Some(a) => try!(FromBencode::from_bencode(a)),
                    None => return Err(MetainfoError::DoesntContain("announce"))
                };

                let created_by = match m.get(&ByteString::from_str("created by")) {
                    Some(a) => try!(FromBencode::from_bencode(a)),
                    None => "".to_string()
                };

                Ok(Metainfo{ announce: announce, created_by: created_by })
            }
            _ => Err(MetainfoError::NotADict)
        }
    }
}

pub fn run() {
    let result = parse_torrent_file("test_data/flagfromserver.torrent");
    match result {
        Ok(s)  => println!("Yay, it worked: {:?}", s),
        Err(e) => println!("Oops, it failed: {}", e)
    }
}

fn parse_torrent_file(filename: &str) -> Result<Metainfo, io::Error> {
    // read the torrent file into a byte vector
    let mut f = try!(File::open(filename));
    let mut v = Vec::new();
    try!(f.read_to_end(&mut v));

    // decode the byte vector into a struct
    let bencode = bencode::from_vec(v).unwrap();
    let result = FromBencode::from_bencode(&bencode).unwrap();

    Ok(result)
}
