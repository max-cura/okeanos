//! Message frame format:
//! ```txt
//! | preamble | COBS FRAME                                      |
//! | preamble | type | length | payload        | crc            |
//!   +0:4       +4:4   +8:4     +12:(length)     +(12+length):4
//! ```

/// Used internally, by [`decode`] and [`encode`].
const COBS_SENTINEL: u8 = 0;

/// Provides functionality for picking out messages from a byte stream
mod decode;
pub use decode::{CobsError, FrameError, FrameHeader, FrameLayer, FrameOutput, PreambleError};

/// Provides functionality for writing out a stream of COBS-stuffed data.
mod encode;
pub use encode::{BufferedEncoder, EncodeState, FrameEncoder, SliceBufferedEncoder};
