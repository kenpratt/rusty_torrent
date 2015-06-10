extern crate bencode;

mod decoder;
mod metainfo;
mod tracker;
mod tracker_response;
mod download;

fn main() {
    match run() {
        Ok(_)  => println!("Yay, it worked!"),
        Err(e) => println!("Oops, it failed: {:?}", e)
    }
}

fn run() -> Result<(), decoder::Error> {
    let filename = "test_data/flagfromserver.torrent";
    let metainfo = try!(metainfo::parse(filename));
    let peers = try!(tracker::get_peers(&metainfo));
    download::download(&metainfo, &peers);
    Ok(())
}
