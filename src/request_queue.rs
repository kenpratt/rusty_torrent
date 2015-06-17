use peer_connection::RequestMetadata;

pub struct RequestQueue {
    requests: Vec<RequestMetadata>,
}

impl RequestQueue {
    pub fn new() -> RequestQueue {
        RequestQueue { requests: vec![] }
    }

    pub fn has(&self, piece_index: u32, block_index: u32) -> bool {
        self.position(piece_index, block_index).is_some()
    }

    pub fn add(&mut self, piece_index: u32, block_index: u32, offset: u32, block_length: u32) -> bool {
        if !self.has(piece_index, block_index) {
            let r = RequestMetadata {
                piece_index: piece_index,
                block_index: block_index,
                offset: offset,
                block_length: block_length,
            };
            self.requests.push(r);
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, piece_index: u32, block_index: u32) -> Option<RequestMetadata> {
        match self.position(piece_index, block_index) {
            Some(i) => {
                let r = self.requests.remove(i);
                Some(r)
            },
            None => None
        }
    }

    fn position(&self, piece_index: u32, block_index: u32) -> Option<usize> {
        self.requests.iter().position(|r| r.matches(piece_index, block_index))
    }

    pub fn len(&self) -> usize {
        self.requests.len()
    }
}
