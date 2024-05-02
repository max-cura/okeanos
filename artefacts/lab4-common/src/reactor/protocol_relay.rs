use core::fmt::{Debug, Formatter};
use core::time::Duration;
use serde::Deserialize;
use theseus_common::theseus::{MessageTypeType, v1};
use theseus_common::theseus::v1::host;
use crate::reactor::{Io, IoEncode, IoEncodeMarker, IoTimeoutsRelative, Logger, Protocol, ProtocolFlow, Reactor};
use crate::timing::Instant;

mod timeouts {
    use crate::timeouts::RateRelativeTimeout;

    pub const ERROR_RECOVERY : RateRelativeTimeout = RateRelativeTimeout::from_bytes(12);
    pub const BYTE_READ : RateRelativeTimeout = RateRelativeTimeout::from_bytes(2);
    pub const SESSION_EXPIRES : RateRelativeTimeout = RateRelativeTimeout::from_bytes(12288 /* 0x3000 */);
    pub const SESSION_EXPIRES_LONG : RateRelativeTimeout = super::super::protocol_theseus::timeouts::SESSION_EXPIRES_LONG;
}

#[derive(Clone, Debug)]
pub struct Program<'a> {
    load_at_address: usize,
    binary: &'a [u8],

    compressed_len: usize,
    decompressed_len: usize,

    compressed_crc: u32,
    decompressed_crc: u32,
}
impl<'a> Program<'a> {
    pub fn new(laa: usize, bin: &'a [u8]) -> Self {
        let crc = crc32fast::hash(bin);
        Self {
            load_at_address: laa,
            binary: bin,
            compressed_len: bin.len(),
            decompressed_len: bin.len(),
            compressed_crc: crc,
            decompressed_crc: crc,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChunkConfig {
    chunk_size: usize,
    num_compressed_chunks: usize,
}
impl ChunkConfig {
    pub fn new() -> Self {
        Self { chunk_size: 0, num_compressed_chunks: 0 }
    }
    pub fn update(&mut self, chunk_size: usize, program: &Program) {
        self.chunk_size = chunk_size;
        self.num_compressed_chunks = (program.compressed_len + chunk_size - 1) / chunk_size;
    }
}

#[derive(Debug, Clone)]
enum SendMessage<'a> {
    ProgramInfo(v1::host::ProgramInfo),
    ProgramReady(v1::host::ProgramReady),
    Chunk(v1::host::Chunk<'a>),
}

impl<'a> SendMessage<'a> {
    pub fn as_encode<'b>(&'b self) -> &'b dyn IoEncode {
        match self {
            SendMessage::ProgramInfo(pi) => pi,
            SendMessage::ProgramReady(pr) => pr,
            SendMessage::Chunk(c) => c,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RelayProtocol {
    program: Program<'static>,
    active_chunk_config: ChunkConfig,

    send: Option<SendMessage<'static>>,
}

impl RelayProtocol {
    pub fn new(
        _reactor: &mut Reactor,
        program: Program<'static>,
    ) -> Self {
        Self {
            program,
            active_chunk_config: ChunkConfig::new(),
            send: None,
        }
    }

    pub fn default_driver_timeouts() -> IoTimeoutsRelative {
        IoTimeoutsRelative {
            error_recovery: timeouts::ERROR_RECOVERY,
            byte_read_timeout: timeouts::BYTE_READ,
            session_timeout: None, //timeouts::SESSION_EXPIRES,
            session_timeout_long: None, //timeouts::SESSION_EXPIRES_LONG,
        }
    }
}

impl IoEncodeMarker for v1::host::ProgramReady {}
impl<'a> IoEncodeMarker for v1::host::Chunk<'a> {}
impl IoEncodeMarker for v1::host::ProgramInfo {}

impl Protocol for RelayProtocol {
    fn protocol_handle(&mut self, reactor: &mut Reactor, logger: &mut dyn Logger, msg: &[u8]) -> ProtocolFlow {
        let (typ, data) = match postcard::take_from_bytes::<MessageTypeType>(msg) {
            Ok(x) => x,
            Err(e) => {
                let _ = logger.writeln_fmt(reactor, format_args!("[device:v1]: failed to deserialize message type: {e}"));
                return ProtocolFlow::Abcon
            }
        };
        self.handle_packet(reactor, logger, typ, data)
    }

    fn protocol_heartbeat(&mut self, reactor: &mut Reactor, io: &mut dyn Io, logger: &mut dyn Logger) -> ProtocolFlow {
        // if self.heartbeat.elapsed(&reactor.peri.SYSTMR) > Duration::from_millis(1000) {
        //     // io.io_queue_message(reactor, logger, &v1::host::ProgramReady);
        //     self.heartbeat = Instant::now(&reactor.peri.SYSTMR);
        // }
        if let Some(msg) = self.send.take() {
            io.io_queue_message(reactor, logger, msg.as_encode());
            // io.io_queue_message(reactor, logger, match msg {
            //     SendMessage::ProgramInfo(pi) => &pi,
            //     SendMessage::ProgramReady(pr) => &pr,
            //     SendMessage::Chunk(ch) => &ch,
            // });
        }

        ProtocolFlow::Continue
    }
}

impl RelayProtocol {
    pub fn handle_packet(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut dyn Logger,
        mtt: MessageTypeType,
        msg_data: &[u8]
    ) -> ProtocolFlow {
        fn resolve<T: for <'a> Deserialize<'a>>(reactor: &mut Reactor, logger: &mut dyn Logger, bytes: &[u8]) -> Option<T> {
            match postcard::from_bytes(&bytes) {
                Ok(x) => Some(x),
                Err(e) => {
                    let _ = logger.writeln_fmt(reactor, format_args!("[relay]: Deserialization error: {e} on bytes: {bytes:?}, ignoring."));
                    None
                }
            }
        }
        match mtt {
            v1::MSG_REQUEST_PROGRAM_INFO => {
                if let Some(rpi) = resolve::<v1::device::RequestProgramInfo>(reactor, logger, msg_data) {
                    logger.writeln_fmt(reactor, format_args!("[relay]: Received RequestProgramInfo"));
                    self.send = Some(SendMessage::ProgramInfo(host::ProgramInfo {
                        load_at_addr: self.program.load_at_address as u32,
                        compressed_len: self.program.compressed_len as u32,
                        decompressed_len: self.program.decompressed_len as u32,
                        compressed_crc: self.program.compressed_crc,
                        decompressed_crc: self.program.decompressed_crc,
                    }));
                }
            }
            v1::MSG_REQUEST_PROGRAM => {
                if let Some(rp) = resolve::<v1::device::RequestProgram>(reactor, logger, msg_data) {
                    logger.writeln_fmt(reactor, format_args!("[relay]: Received RequestProgram"));
                    let compressed_crc_ok = rp.verify_compressed_crc == self.program.compressed_crc;
                    let decompressed_crc_ok = rp.verify_decompressed_crc == self.program.decompressed_crc;
                    if !compressed_crc_ok {
                        logger.writeln_fmt(reactor, format_args!("[relay]: Compressed CRC mismatch: expected {} received {}", self.program.compressed_crc, rp.verify_compressed_crc));
                    }
                    if !decompressed_crc_ok {
                        logger.writeln_fmt(reactor, format_args!("[relay]: Decompressed CRC mismatch: expected {} received {}", self.program.decompressed_crc, rp.verify_decompressed_crc));
                    }
                    if compressed_crc_ok && decompressed_crc_ok {
                        self.active_chunk_config.update(rp.chunk_size as usize, &self.program);
                        // progress bar init
                        self.send = Some(SendMessage::ProgramReady(host::ProgramReady));
                    } else {
                        logger.writeln_fmt(reactor, format_args!("[relay]: Bad CRCs, ignoring."));
                    }
                }
            }
            v1::MSG_REQUEST_CHUNK => {
                if let Some(rc) = resolve::<v1::device::RequestChunk>(reactor, logger, msg_data) {
                    logger.writeln_fmt(reactor, format_args!("[relay]: Received RequestChunk"));
                    let chunk_begin = rc.chunk_no as usize * self.active_chunk_config.chunk_size;
                    let chunk_end = (chunk_begin + self.active_chunk_config.chunk_size).min(self.program.binary.len());

                    // progress bar update

                    self.send = Some(SendMessage::Chunk(v1::host::Chunk {
                        chunk_no: rc.chunk_no,
                        data: &self.program.binary[chunk_begin..chunk_end],
                    }))
                }
            }
            v1::MSG_BOOTING => {
                logger.writeln_fmt(reactor, format_args!("[relay]: Device booted successfully!"));
            }
            _ => {
                logger.writeln_fmt(reactor, format_args!("[relay]: Unrecognized message type: {mtt}, ignoring."));
            }
        }
        ProtocolFlow::Continue
    }
}