mod metainfo;
mod tracker;

use metainfo::MetainfoError;

fn main() {
    match run() {
        Ok(_)  => println!("Yay, it worked!"),
        Err(e) => println!("Oops, it failed: {:?}", e)
    }
}

fn run() -> Result<(), MetainfoError> {
    let filename = "test_data/flagfromserver.torrent";
    let metainfo = try!(metainfo::parse(filename));
    tracker::run(metainfo);
    Ok(())
}
