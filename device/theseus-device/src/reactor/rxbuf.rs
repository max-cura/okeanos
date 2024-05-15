use crate::reactor::ReceiveError;

/// Receive buffer with the data from a COBS frame.
pub struct FrameDataBuffer {
    underlying_storage: &'static mut [u8],
    cursor: usize,
}

impl FrameDataBuffer {
    pub fn new(underlying_storage: &'static mut [u8]) -> Self {
        Self { underlying_storage, cursor: 0 }
    }
    pub fn clear(&mut self) {
        self.cursor = 0;
        self.underlying_storage.iter_mut().for_each(|x| *x = 0);
    }
    pub fn push_byte(&mut self, b: u8) -> Result<(), ReceiveError> {
        if self.cursor >= self.underlying_storage.len() {
            // that's not great
            return Err(ReceiveError::BufferOverflow);
        }
        self.underlying_storage[self.cursor] = b;
        self.cursor += 1;
        Ok(())
    }
    pub fn finalize(&mut self) -> &mut [u8] {
        let end = self.cursor;
        self.cursor = 0;
        &mut self.underlying_storage[..end]
    }
}
