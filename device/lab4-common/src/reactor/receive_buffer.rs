/// Receive buffer with no decode logic. Single-message.
pub struct ReceiveBuffer {
    underlying_storage: &'static mut [u8],
    cursor: usize,
}

impl ReceiveBuffer {
    pub fn new(underlying_storage: &'static mut [u8]) -> Self {
        Self { underlying_storage, cursor: 0 }
    }
    pub fn clear(&mut self) {
        self.cursor = 0;
        self.underlying_storage.iter_mut().for_each(|x| *x = 0);
    }
    pub fn push_byte(&mut self, b: u8) -> bool {
        if self.cursor >= self.underlying_storage.len() {
            // that's not great
            return false
        }
        self.underlying_storage[self.cursor] = b;
        self.cursor += 1;
        true
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.underlying_storage[..self.cursor]
    }
    pub fn len(&self) -> usize {
        self.underlying_storage.len()
    }
}
