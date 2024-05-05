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
    // not strictly necessary but eh
    pub fn reset(&mut self) {
        self.last_jump = 0;
        self.bytes_since_last_jump = 0;
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
            // Cases:
            //  b = 0 -> finished
            //  b ≠ 0, LJ<255 -> byte
            //  b ≠ 0, LJ=255 -> skip
            let prev_jump = self.last_jump;
            self.last_jump = byte_raw as usize;
            self.bytes_since_last_jump = 0;
            if byte_raw == SENTINEL {
                self.reset();
                FeedState::PacketFinished
            } else {
                if prev_jump < 0xff {
                    FeedState::Byte(SENTINEL)
                } else {
                    FeedState::Pass
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct BufferedEncoder<'a> {
    internal: &'a mut [u8],
    xor: u8,
}

impl<'a> BufferedEncoder<'a> {
    pub fn with_buffer(underlying_buffer: &'a mut [u8], xor: u8) -> Option<Self> {
        (underlying_buffer.len() == 255).then(|| Self {
            internal: underlying_buffer,
            xor,
        })
    }
    pub fn packet(&mut self) -> PacketEncoder {
        PacketEncoder::with_buffer(self.internal, self.xor)
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
    inner: PacketEncoderInner<'a>,
    xor: u8,
}

impl<'a> PacketEncoder<'a> {
    pub fn with_buffer(underlying_buffer: &'a mut [u8], xor: u8) -> Option<Self> {
        Some(Self { inner: PacketEncoderInner::with_buffer(underlying_buffer)?, xor })
    }
    pub fn add_byte(&mut self, x: u8) -> EncodeState {
        match self.inner.add_byte(x) {
            EncodeStateInner::Buf(b) => {
                b.iter_mut().for_each(|x| *x ^= self.xor);
                EncodeState::Buf(&b[..])
            }
            EncodeStateInner::Pass => EncodeState::Pass
        }
    }
    pub fn finish(self) -> &'a [u8] {
        let b = self.inner.finish();
        b.iter_mut().for_each(|x| *x ^= self.xor);
        &b[..]
    }
}

#[derive(Debug)]
pub enum EncodeStateInner<'a> {
    Buf(&'a mut [u8]),
    Pass,
}

pub struct PacketEncoderInner<'a> {
    buf: &'a mut [u8],
    curs: usize,
}

impl<'a> PacketEncoderInner<'a> {
    pub fn with_buffer(underlying_buffer: &'a mut [u8]) -> Option<Self> {
        (underlying_buffer.len() == 255).then(|| Self {
            buf: underlying_buffer,
            curs: 1,
        })
    }
    pub fn add_byte(&mut self, x: u8) -> EncodeStateInner {
        if self.curs == 254 {
            // 0 [1 ... 254] 255
            //           ^- we are here
            // Case 1:
            //  x = 0 -> buf[0] = fe
            //  x ≠ 0 -> buf[0] = ff, buf[fe]=x
            if x == SENTINEL {
                self.buf[0] = 0xfe;
                self.curs = 1;
                return EncodeStateInner::Buf(&mut self.buf[0..254])
            } else {
                self.buf[0] = 0xff;
                self.buf[0xfe] = x;
                self.curs = 1;
                return EncodeStateInner::Buf(&mut self.buf[0..255])
            }
            // OLD AND NOT WORKING:
            // self.buf[0] = 0xff;
            // self.curs = 1;
            // return EncodeStateInner::Buf(&mut self.buf[0..255])
        }
        if x != SENTINEL {
            self.buf[self.curs] = x;
            self.curs += 1;
            EncodeStateInner::Pass
        } else {
            self.buf[0] = self.curs as u8;
            let saved_cursor = self.curs as usize;
            self.curs = 1;
            EncodeStateInner::Buf(&mut self.buf[0..saved_cursor])
        }
    }

    pub fn finish(mut self) -> &'a mut [u8] {
        self.buf[self.curs] = SENTINEL;
        self.buf[0] = self.curs as u8;
        self.curs += 1;
        &mut self.buf[0..self.curs]
    }
}