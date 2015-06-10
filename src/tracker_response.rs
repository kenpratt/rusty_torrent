extern crate sha1;

use bencode;
use bencode::{Bencode, FromBencode};
use bencode::util::ByteString;
use std::net::Ipv4Addr;

use decoder;

#[derive(PartialEq, Debug)]
pub struct TrackerResponse {
    pub interval: u32,
    pub min_interval: Option<u32>,
    pub complete: u32,
    pub incomplete: u32,
    pub downloaded: u32,
    pub peers: Vec<Peer>,
}

impl TrackerResponse {
    pub fn parse(bytes: &[u8]) -> Result<TrackerResponse, decoder::Error> {
        let bencode = try!(bencode::from_buffer(bytes));
        FromBencode::from_bencode(&bencode)
    }
}

impl FromBencode for TrackerResponse {
    type Err = decoder::Error;

    fn from_bencode(bencode: &bencode::Bencode) -> Result<TrackerResponse, decoder::Error> {
        match bencode {
            &Bencode::Dict(ref m) => {
                let peers = get_field_as_bytes!(m, "peers").chunks(6).map(Peer::from_bytes).collect();

                let response = TrackerResponse{
                    interval: get_field!(m, "interval"),
                    min_interval: get_optional_field!(m, "min interval"),
                    complete: get_field!(m, "complete"),
                    incomplete: get_field!(m, "incomplete"),
                    downloaded: get_field!(m, "downloaded"),
                    peers: peers,
                };
                Ok(response)
            }
            _ => Err(decoder::Error::NotADict)
        }
    }
}

#[derive(PartialEq, Debug)]
pub struct Peer {
    pub ip: Ipv4Addr,
    pub port: u16,
}

impl Peer {
    fn from_bytes(v: &[u8]) -> Peer {
        let ip = Ipv4Addr::new(v[0], v[1], v[2], v[3]);
        let port = (v[4] as u16) * 256 + (v[5] as u16);
        Peer{ ip: ip, port: port }
    }
}
