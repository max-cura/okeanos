const SENTINEL : u8 = 0x00;

#[derive(Debug, Clone)]
pub struct LineDecoder {
    bytes_since_last_jump: usize,
    last_jump: usize,
}

#[derive(Debug, Copy, Clone)]
pub enum FeedState {
    PacketFinished,
    Byte(u8),
    Pass,
}

impl LineDecoder {
    pub fn new() -> Self {
        Self {
            bytes_since_last_jump: 0,
            last_jump: 0,
        }
    }
    pub fn feed(&mut self, byte_raw: u8) -> FeedState {
        // packet start
        if self.last_jump == 0 {
            // assert(byte_raw != 0)
            self.last_jump = byte_raw as usize;
            self.bytes_since_last_jump = 0;
            return FeedState::Pass
        }
        self.bytes_since_last_jump += 1;
        if self.bytes_since_last_jump < self.last_jump {
            // assert(byte_raw != 0
            FeedState::Byte(byte_raw)
        } else {
            self.last_jump = byte_raw as usize;
            self.bytes_since_last_jump = 0;
            if byte_raw == SENTINEL {
                FeedState::PacketFinished
            } else {
                FeedState::Byte(SENTINEL)
            }
        }
    }
}

#[derive(Debug)]
pub struct BufferedEncoder<'a> {
    internal: &'a mut [u8],
}

impl<'a> BufferedEncoder<'a> {
    pub fn with_buffer(underlying_buffer: &'a mut [u8]) -> Option<Self> {
        (underlying_buffer.len() == 254).then(|| Self {
            internal: underlying_buffer,
        })
    }
    pub fn packet(&mut self) -> PacketEncoder {
        PacketEncoder::with_buffer(self.internal)
            // We guarantee that the buffer is of the correct length
            .unwrap()
    }
}

#[derive(Debug)]
pub enum EncodeState<'a> {
    Buf(&'a [u8]),
    Pass,
}

pub struct PacketEncoder<'a> {
    buf: &'a mut [u8],
    curs: usize,
}

impl<'a> PacketEncoder<'a> {
    pub fn with_buffer(underlying_buffer: &'a mut [u8]) -> Option<Self> {
        (underlying_buffer.len() == 254).then(|| Self {
            buf: underlying_buffer,
            curs: 1,
        })
    }
    pub fn add_byte(&mut self, x: u8) -> EncodeState {
        if self.curs == 254 {
            // 0 [1 ... 254] 255
            //           ^- we are here; need another overhead
            self.buf[0] = 0xff;
            self.curs = 1;
            return EncodeState::Buf(&self.buf[0..255])
        }
        if x != SENTINEL {
            self.buf[self.curs] = x;
            self.curs += 1;
            EncodeState::Pass
        } else {
            self.buf[0] = self.curs as u8;
            let saved_cursor = self.curs as usize;
            self.curs = 1;
            EncodeState::Buf(&self.buf[0..saved_cursor])
        }
    }

    pub fn finish(mut self) -> &'a [u8] {
        self.buf[self.curs] = SENTINEL;
        self.buf[0] = self.curs as u8;
        self.curs += 1;
        &self.buf[0..self.curs]
    }
}