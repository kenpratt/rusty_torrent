extern crate hyper;
extern crate url;

use metainfo::Metainfo;
use std::io::Read;
use self::url::percent_encoding::{percent_encode, FORM_URLENCODED_ENCODE_SET};
use self::hyper::Client;
use self::hyper::header::Connection;
use tracker_response::{TrackerResponse,Peer};
use decoder;

pub fn get_peers(metainfo: &Metainfo) ->Result<Vec<Peer>, decoder::Error> {
    let length_string = metainfo.info.length.to_string();
    let encoded_info_hash = percent_encode(&metainfo.info_hash, FORM_URLENCODED_ENCODE_SET);
    let params = vec![("left", length_string.as_ref()),
                      ("info_hash", encoded_info_hash.as_ref()),
                      ("downloaded", "0"),
                      ("uploaded", "0"),
                      ("event", "started"),
                      ("peer_id", "-TZ-0000-00000000001"),
                      ("port", "6881")];
    let url = format!("{}?{}", metainfo.announce, encode_query_params(&params));

    let mut client = Client::new();
    let mut http_res = client.get(&url).header(Connection::close()).send().unwrap();

    let mut body = Vec::new();
    http_res.read_to_end(&mut body).unwrap();

    let res = TrackerResponse::parse(&body).unwrap();
    Ok(res.peers)
}

fn encode_query_params(params: &[(&str, &str)]) -> String {
    let param_strings: Vec<String> = params.iter().map(|&(k, v)| format!("{}={}", k, v)).collect();
    param_strings.connect("&")
}
