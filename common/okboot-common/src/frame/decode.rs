use crate::frame::COBS_SENTINEL;
use crate::MessageType;
use core::hash::Hasher;
use core::intrinsics::{likely, unlikely};
use thiserror::Error;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum PreambleState {
    Initial,
    Preamble1,
    Preamble2,
    Preamble3,
    Finished,
}
#[derive(Clone, Copy, Debug, Error)]
#[error("Found byte {wrong_byte:02x} in state {state:?}")]
pub struct PreambleError {
    state: PreambleState,
    wrong_byte: u8,
}
#[derive(Debug)]
struct PreambleDecoder {
    state: PreambleState,
}
impl PreambleDecoder {
    pub fn new() -> Self {
        Self {
            state: PreambleState::Initial,
        }
    }
    fn reset(&mut self) {
        self.state = PreambleState::Initial
    }
    fn feed(&mut self, byte: u8) -> Result<bool, PreambleError> {
        let state;
        match (self.state, byte) {
            (PreambleState::Initial, 0x55) => {
                state = PreambleState::Preamble1;
            }
            (PreambleState::Preamble1, 0x55) => {
                state = PreambleState::Preamble2;
            }
            (PreambleState::Preamble2, 0x55) => {
                state = PreambleState::Preamble3;
            }
            (PreambleState::Preamble3, 0x5e) => {
                state = PreambleState::Finished;
            }
            (PreambleState::Preamble3, 0x55) => {
                state = PreambleState::Preamble3;
            }
            (state, wrong_byte) => {
                self.state = PreambleState::Initial;
                return Err(PreambleError { state, wrong_byte });
            }
        }
        Ok(state == PreambleState::Finished)
    }
}
/// Decodes the preamble from a message stream.
#[derive(Debug)]
pub struct PreambleLayer {
    preamble_decoder: PreambleDecoder,
}
impl PreambleLayer {
    pub fn new() -> Self {
        Self {
            preamble_decoder: PreambleDecoder::new(),
        }
    }
    pub fn reset(&mut self) {
        self.preamble_decoder.reset();
    }
    /// Returns `Ok(true)` if `byte` was the last byte of a complete and valid preamble.
    /// Returns `Ok(false)` if `byte` is the next byte of a valid preamble.
    /// Returns `Err` if `byte` is not part of a valid preamble.
    pub fn poll(&mut self, byte: u8) -> Result<bool, PreambleError> {
        self.preamble_decoder.feed(byte)
    }
}

// This can only occur if there was a premature sentinel byte, or no sentinel byte was found.
#[derive(Debug, Clone, Copy, Error)]
#[error("At position {frame_loc} found byte {found:02x}, with last_jump={last_jump} and bytes_since_last_jump={bytes_since_last_jump}")]
pub struct CobsError {
    frame_loc: usize,
    bytes_since_last_jump: usize,
    last_jump: usize,
    found: u8,
}
#[derive(Debug)]
pub enum CobsState {
    Skip,
    Byte(u8),
    Finished,
}
#[derive(Debug)]
struct CobsDecoder {
    frame_loc: usize,
    bytes_since_last_jump: usize,
    last_jump: usize,
    xor: u8,
}
impl CobsDecoder {
    pub fn new(xor: u8) -> Self {
        Self {
            frame_loc: 0,
            bytes_since_last_jump: 0,
            last_jump: 0,
            xor,
        }
    }
    fn reset(&mut self) {
        self.frame_loc = 0;
        self.bytes_since_last_jump = 0;
        self.last_jump = 0;
    }
    fn error(&self, found: u8) -> CobsError {
        CobsError {
            frame_loc: self.frame_loc,
            bytes_since_last_jump: self.bytes_since_last_jump,
            last_jump: self.last_jump,
            found,
        }
    }
    pub fn poll(&mut self, byte: u8) -> Result<CobsState, CobsError> {
        let byte = byte ^ self.xor;
        if self.last_jump == 0 {
            // CASE: packet start

            // Byte should not be zero
            if unlikely(byte == COBS_SENTINEL) {
                return Err(self.error(byte));
            }
            // Impossible case
            if unlikely(self.bytes_since_last_jump != self.last_jump) {
                return Err(self.error(byte));
            }

            self.last_jump = byte as usize;
            self.bytes_since_last_jump = 0;
            return Ok(CobsState::Skip);
        }
        // Okay, tick forward one
        self.bytes_since_last_jump += 1;
        if likely(self.bytes_since_last_jump < self.last_jump) {
            // CASE: in between jumps
            if unlikely(byte == COBS_SENTINEL) {
                Err(self.error(byte))
            } else {
                Ok(CobsState::Byte(byte))
            }
        } else {
            // CASES:
            //  b = 0 -> finished
            //  b ≠ 0, LJ<255 -> byte
            //  b ≠ 0, LJ=255 -> skip
            let prev_jump = self.last_jump;
            self.last_jump = byte as usize;
            self.bytes_since_last_jump = 0;
            if unlikely(byte == 0) {
                self.reset();
                Ok(CobsState::Finished)
            } else {
                if likely(prev_jump < 0xff) {
                    Ok(CobsState::Byte(0))
                } else {
                    Ok(CobsState::Skip)
                }
            }
        }
    }
}
pub enum CobsStateCRC {
    Skip,
    Byte(u8),
    Finished(u32),
}
#[derive(Debug)]
pub struct CobsLayer {
    cobs_decoder: CobsDecoder,
    crc: crc32fast::Hasher,
}
impl CobsLayer {
    pub fn with_xor(xor: u8) -> Self {
        Self {
            cobs_decoder: CobsDecoder::new(xor),
            crc: crc32fast::Hasher::new(),
        }
    }
    pub fn reset(&mut self) {
        self.cobs_decoder.reset();
        self.crc = crc32fast::Hasher::new();
    }
    pub fn poll(&mut self, byte: u8) -> Result<CobsStateCRC, CobsError> {
        self.cobs_decoder
            .poll(byte)
            .map(|cobs_state| match cobs_state {
                CobsState::Byte(b) => {
                    self.crc.write_u8(b);
                    CobsStateCRC::Byte(b)
                }
                CobsState::Finished => CobsStateCRC::Finished(
                    core::mem::replace(&mut self.crc, crc32fast::Hasher::new()).finalize(),
                ),
                CobsState::Skip => CobsStateCRC::Skip,
            })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct FrameHeader {
    pub message_type: MessageType,
    pub payload_len: usize,
}
#[derive(Debug, Copy, Clone, Error)]
pub enum FrameError {
    #[error("Invalid message type: {0}")]
    InvalidType(u32),
    #[error("Invalid CRC: calculated {0}, expected {1}")]
    InvalidCRC(u32, u32),
    #[error("Error during COBS decoding: {0}")]
    Cobs(#[source] CobsError),
    #[error("Error during preamble: {0}")]
    Preamble(#[source] PreambleError),
    #[error("COBS frame longer than expected. Frame header:{0:?}, found byte: {1:02x}")]
    Overrun(FrameHeader, u8),
    #[error("Header cut off at position {1} after receiving: {0:?}")]
    HeaderCutoff([u8; 8], usize),
    #[error("Payload cut off at position {1} with frame header: {0:?}")]
    PayloadCutoff(FrameHeader, usize),
    #[error("CRC32 cut off at position {1} after receiving: {0:?}")]
    CrcCutoff([u8; 4], usize),
}
#[derive(Debug, Copy, Clone)]
enum FrameState {
    Preamble,
    Packet(usize),
}
#[derive(Debug)]
pub struct FrameLayer {
    preamble_layer: PreambleLayer,
    cobs_layer: CobsLayer,

    header_bytes: [u8; 8],
    crc_bytes: [u8; 4],
    frame_header: Option<FrameHeader>,
    decode_state: FrameState,
}
#[derive(Debug, Copy, Clone)]
pub enum FrameOutput {
    Skip,
    Header(FrameHeader),
    Payload(u8),
    Finished,
}
impl FrameLayer {
    pub fn new(cobs_xor: u8) -> Self {
        Self {
            preamble_layer: PreambleLayer::new(),
            cobs_layer: CobsLayer::with_xor(cobs_xor),

            header_bytes: [0; 8],
            crc_bytes: [0; 4],
            frame_header: None,
            decode_state: FrameState::Preamble,
        }
    }
    fn decode_header_bytes(&self) -> Result<FrameHeader, FrameError> {
        let message_type = u32::from_le_bytes(self.header_bytes[0..4].try_into().unwrap());
        let payload_len = u32::from_le_bytes(self.header_bytes[4..8].try_into().unwrap());
        let message_type = MessageType::try_from(message_type)
            .map_err(|_| FrameError::InvalidType(message_type))?;
        const _: () = assert!(size_of::<u32>() <= size_of::<usize>());
        let payload_len = payload_len as usize;
        Ok(FrameHeader {
            message_type,
            payload_len,
        })
    }
    pub fn reset(&mut self) {
        self.preamble_layer.reset();
        self.cobs_layer.reset();

        self.header_bytes.iter_mut().for_each(|x| *x = 0);
        self.crc_bytes.iter_mut().for_each(|x| *x = 0);
        self.frame_header = None;
        self.decode_state = FrameState::Preamble;
    }
    pub fn poll(&mut self, byte: u8) -> Result<FrameOutput, FrameError> {
        match self.decode_state {
            FrameState::Preamble => {
                let finished = self
                    .preamble_layer
                    .poll(byte)
                    .map_err(FrameError::Preamble)?;
                if finished {
                    self.decode_state = FrameState::Packet(0);
                }
                Ok(FrameOutput::Skip)
            }
            FrameState::Packet(i) => {
                match self.cobs_layer.poll(byte).map_err(FrameError::Cobs)? {
                    CobsStateCRC::Skip => Ok(FrameOutput::Skip),
                    CobsStateCRC::Byte(byte) => {
                        self.decode_state = FrameState::Packet(i + 1);
                        if i <= 7 {
                            self.header_bytes[i] = byte;
                            if i == 7 {
                                let frame_header = self.decode_header_bytes()?;
                                self.frame_header = Some(frame_header);
                                Ok(FrameOutput::Header(frame_header))
                            } else {
                                Ok(FrameOutput::Skip)
                            }
                        } else {
                            let packet_finishes_at_index =
                                4 + 4 + self.frame_header.as_ref().unwrap().payload_len;

                            if i < packet_finishes_at_index {
                                Ok(FrameOutput::Payload(byte))
                            } else if i < packet_finishes_at_index + 4 {
                                let offset = i - packet_finishes_at_index;
                                self.crc_bytes[offset] = byte;
                                Ok(FrameOutput::Skip)
                            } else {
                                Err(FrameError::Overrun(self.frame_header.take().unwrap(), byte))
                            }
                        }
                    }
                    CobsStateCRC::Finished(crc) => {
                        if i <= 7 {
                            return Err(FrameError::HeaderCutoff(self.header_bytes, i));
                        }
                        let frame_header = self.frame_header.as_ref().unwrap();
                        // TYPE + PAYLOAD_LEN + [payload] + CRC32
                        let expected_crc_begin = 4 + 4 + frame_header.payload_len;
                        let expected_cobs_payload_len = 4 + 4 + frame_header.payload_len + 4;
                        if i < expected_crc_begin {
                            Err(FrameError::PayloadCutoff(*frame_header, i))
                        } else if i < expected_cobs_payload_len {
                            Err(FrameError::CrcCutoff(self.crc_bytes, i))
                        } else {
                            let frame_crc = u32::from_le_bytes(self.crc_bytes);
                            if frame_crc != crc {
                                Err(FrameError::InvalidCRC(crc, frame_crc))
                            } else {
                                Ok(FrameOutput::Finished)
                            }
                        }
                    }
                }
            }
        }
    }
}
