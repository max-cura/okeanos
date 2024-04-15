const SENTINEL : u8 = 0x00;

pub struct LineDecoder {
    bytes_since_last_jump: usize,
    last_jump: usize,
}

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

pub struct BufferedEncoder {
    internal: [u8; 256],
}

impl BufferedEncoder {
    pub fn new() -> Self {
        Self {
            internal: [0; 256],
        }
    }
    pub fn packet(&mut self) -> PacketEncoder {
        PacketEncoder::new(&mut self.internal)
    }
}


pub enum EncodeState<'a> {
    Buf(&'a [u8]),
    Pass,
}

pub struct PacketEncoder<'a> {
    buf: &'a mut [u8; 256],
    curs: usize,
}

impl<'a> PacketEncoder<'a> {
    pub fn new(underlying_buffer: &'a mut [u8; 256]) -> Self {
        Self {
            buf: underlying_buffer,
            curs: 1,
        }
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

    pub fn finish(self) -> &'a [u8] {
        self.buf[self.curs] = SENTINEL;
        self.buf[0] = self.curs as u8;
        &self.buf[0..self.curs]
    }
}