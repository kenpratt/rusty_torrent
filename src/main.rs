mod metainfo;

fn main() {
    let filename = "test_data/flagfromserver.torrent";
    match metainfo::parse(filename) {
        Ok(s)  => println!("Yay, it worked: {:?}", s),
        Err(e) => println!("Oops, it failed: {:?}", e)
    }
}
