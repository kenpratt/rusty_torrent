extern crate hyper;
extern crate url;

use metainfo::Metainfo;
use std::io;
use self::url::percent_encoding::{percent_encode, FORM_URLENCODED_ENCODE_SET};
use self::hyper::Client;
use self::hyper::header::Connection;

pub fn run(metainfo: Metainfo) {
    let length_string = metainfo.info.length.to_string();
    let encoded_info_hash = percent_encode(&metainfo.info_hash, FORM_URLENCODED_ENCODE_SET);
    let params = vec![("left", length_string.as_ref()),
                      ("info_hash", encoded_info_hash.as_ref()),
                      ("downloaded", "0"),
                      ("uploaded", "0"),
                      ("event", "started"),
                      ("peer_id", "-TZ-0000-00000000000"),
                      ("port", "6881")];
    let url = format!("{}?{}", metainfo.announce, encode_query_params(&params));

    let mut client = Client::new();
    let mut res = client.get(&url).header(Connection::close()).send().unwrap();

    println!("Response: {}", res.status);
    println!("Headers:\n{}", res.headers);
    io::copy(&mut res, &mut io::stdout()).unwrap();
}

fn encode_query_params(params: &[(&str, &str)]) -> String {
    let param_strings: Vec<String> = params.iter().map(|&(k, v)| format!("{}={}", k, v)).collect();
    param_strings.connect("&")
}
