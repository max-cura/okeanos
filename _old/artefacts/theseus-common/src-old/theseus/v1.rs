use core::time::Duration;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[repr(C, u32)]
pub enum MessageContent<'a> {
    SetProtocolVersion {
        #[serde(with = "postcard::fixint::le")]
        version: u32,
    } = 0xff00_11ff,

    PrintMessageRPC {
        // PROBLEM 1
        message: &'a str,
    } = 0xfe00_0001,

    SetBaudRateRPC {
        #[serde(with = "postcard::fixint::le")]
        baud_rate: u32,
    } = 101,
    BaudRateAck {
        possible: bool,
    } = 102,
    BaudRateReady = 103,

    RequestProgramInfoRPC = 201,
    ProgramInfo {
        #[serde(with = "postcard::fixint::le")]
        load_at_address: u32,
        #[serde(with = "postcard::fixint::le")]
        program_size: u32,
        #[serde(with = "postcard::fixint::le")]
        program_crc32: u32,
    } = 202,

    RequestProgramRPC {
        #[serde(with = "postcard::fixint::le")]
        crc_retransmission: u32,
        #[serde(with = "postcard::fixint::le")]
        chunk_size: u32,
    } = 301,
    ProgramReady = 302,
    ReadyForChunk {
        #[serde(with = "postcard::fixint::le")]
        chunk_no: u32,
    } = 303,
    ProgramChunk {
        #[serde(with = "postcard::fixint::le")]
        chunk_no: u32,
        // PROBLEM 2
        data: &'a [u8],
    } = 304,
    ProgramReceived = 305,
}

// 1111 1111 1010 1010 0111 0111 0101 0101
pub const MESSAGE_PRECURSOR: u32 = 0xffaa7755;

/// Timeout while blocking on incoming message.
pub const RECEIVE_TIMEOUT: Duration = Duration::from_millis(10);
/// Timeout before either side decides to fall back to default baud rate.
pub const SET_BAUD_RATE_TIMEOUT: Duration = Duration::from_millis(50);
/// Number of retries for retry-capped messages.
pub const RETRY_ATTEMPTS_CAP: usize = 10;

pub const DEVICE_PROTOCOL_RESET_TIMEOUT : Duration = Duration::from_millis(100);
