extern crate bencode;

use self::bencode::{FromBencode, Bencode, StringFromBencodeError};
use self::bencode::util::ByteString;
use std::fs::File;
use std::io::Read;
use std::io;

#[derive(PartialEq, Debug)]
struct Metainfo {
    announce: String,
    created_by: String,
}

#[derive(Debug)]
enum MyError {
    NotADict,
    DoesntContain(&'static str),
    ANotAString(StringFromBencodeError),
}

impl FromBencode for Metainfo {
    type Err = MyError;

    fn from_bencode(bencode: &bencode::Bencode) -> Result<Metainfo, MyError> {
        match bencode {
            &Bencode::Dict(ref m) => {
                let announce = try!(match m.get(&ByteString::from_str("announce")) {
                    Some(a) => FromBencode::from_bencode(a).map_err(MyError::ANotAString),
                    None => Err(MyError::DoesntContain("announce"))
                });

                let created_by = try!(match m.get(&ByteString::from_str("created by")) {
                    Some(a) => FromBencode::from_bencode(a).map_err(MyError::ANotAString),
                    None => Ok("".to_string())
                });

                Ok(Metainfo{ announce: announce, created_by: created_by })
            }
            _ => Err(MyError::NotADict)
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
