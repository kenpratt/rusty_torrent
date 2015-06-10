extern crate bencode;

mod decoder;
mod metainfo;
mod tracker;

use decoder::Error;

fn main() {
    match run() {
        Ok(_)  => println!("Yay, it worked!"),
        Err(e) => println!("Oops, it failed: {:?}", e)
    }
}

fn run() -> Result<(), decoder::Error> {
    let filename = "test_data/flagfromserver.torrent";
    let metainfo = try!(metainfo::parse(filename));
    tracker::run(metainfo);
    Ok(())
}
