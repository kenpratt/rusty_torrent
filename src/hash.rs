extern crate sha1;

pub type Sha1 = Vec<u8>;

pub fn calculate_sha1(input: &[u8]) -> Sha1 {
    let mut hasher = sha1::Sha1::new();
    hasher.update(input);
    hasher.digest()
}
