extern crate bencode;

mod decoder;
mod download;
mod hash;
mod metainfo;
mod tracker;
mod tracker_response;

fn main() {
    match run() {
        Ok(_)  => println!("Yay, it worked!"),
        Err(e) => println!("Oops, it failed: {:?}", e)
    }
}

fn run() -> Result<(), tracker::Error> {
    let filename = "test_data/flagfromserver.torrent";
    let metainfo = try!(metainfo::parse(filename));
    let peers = try!(tracker::get_peers(&metainfo));
    download::download(&metainfo, &peers);
    Ok(())
}
