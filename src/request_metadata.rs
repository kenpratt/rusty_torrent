#[derive(Debug)]
pub struct RequestMetadata {
    pub piece_index: u32,
    pub block_index: u32,
    pub offset: u32,
    pub block_length: u32,
}

impl RequestMetadata {
    pub fn matches(&self, piece_index: u32, block_index: u32) -> bool {
        self.piece_index == piece_index && self.block_index == block_index
    }
}
