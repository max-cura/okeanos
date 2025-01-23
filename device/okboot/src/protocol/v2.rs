use crate::buf::{FrameSink, SendError};
use crate::protocol::{ProtocolStatus, Timeouts};
use crate::rpc_println;
use crate::stub::flat_binary::{Integrity, Relocation};
use bcm2835_lpa::Peripherals;
use core::fmt::Debug;
use core::time::Duration;
use miniz_oxide::inflate::stream::InflateState;
use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};
use okboot_common::frame::FrameHeader;
use okboot_common::host::{Chunk, FormatDetails, Metadata};
use okboot_common::{device, host, MessageType};
use quartz::device::bcm2835::timing::Instant;
use thiserror::Error;

const CHUNK_SIZE: usize = 0x1000;
mod timeouts {
    use crate::timeouts::RateRelativeTimeout;

    pub const TRY_RESEND: RateRelativeTimeout = RateRelativeTimeout::from_bytes(0x300);
    // 150% CHUNK_SIZE
    pub const TRY_RESEND_CHUNK: RateRelativeTimeout =
        RateRelativeTimeout::from_bytes(super::CHUNK_SIZE * 16);
    pub const BUFFER_RETRY: RateRelativeTimeout = RateRelativeTimeout::from_bytes(0x80);
}
#[derive(Debug, Copy, Clone)]
struct V1Timeouts {
    try_resend: Duration,
    buffer_retry: Duration,
    try_resend_chunk: Duration,
}

impl V1Timeouts {
    pub fn new_8n1(baud: u32) -> Self {
        Self {
            try_resend: timeouts::TRY_RESEND.at_baud_8n1(baud),
            buffer_retry: timeouts::BUFFER_RETRY.at_baud_8n1(baud),
            try_resend_chunk: timeouts::TRY_RESEND_CHUNK.at_baud_8n1(baud),
        }
    }
}

#[derive(Debug)]
enum S {
    /// expect: [`MetadataAck`], send: [`MetadataReq`]
    RequestMetadata,
    /// expect: [`MetadataAckAck`], send: [`MetadataAck`]
    AckMetadata(Metadata),
    /// expect: [`Chunk`], send: [`ChunkReq`]
    RequestChunk {
        which: usize,
        count: usize,
        loader: LoaderEnum,
    },
    /// expect: [`BootingAck`], send: [`Booting`]
    Boot { booter: Booter },
}

pub struct V2 {
    state: S,

    once: bool,
    retry_buffer: bool,
    heartbeat: Instant,

    baud: u32,
    timeouts: V1Timeouts,

    inflate_state: InflateState,
    remainder: usize,
}
impl Debug for V2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("V2")
            .field("state", &self.state)
            .field("once", &self.once)
            .field("retry_buffer", &self.retry_buffer)
            .field("heartbeat", &self.heartbeat)
            .field("baud", &self.baud)
            .field("timeouts", &self.timeouts)
            // .field("inflate_state", "<opaque>")
            .finish()
    }
}

impl V2 {
    pub fn new(peripherals: &Peripherals, baud: u32) -> Self {
        Self {
            state: S::RequestMetadata,
            once: true,
            retry_buffer: false,
            heartbeat: Instant::now(&peripherals.SYSTMR),
            baud,
            timeouts: V1Timeouts::new_8n1(baud),
            inflate_state: InflateState::new(DataFormat::Raw),
            remainder: 0,
        }
    }
}

impl super::Protocol for V2 {
    fn handle_packet(
        &mut self,
        frame_header: FrameHeader,
        payload: &[u8],
        frame_sink: &mut FrameSink,
        timeouts: &mut Timeouts,
        peripherals: &Peripherals,
        inflate_buffer: &mut [u8],
    ) -> ProtocolStatus {
        match frame_header.message_type {
            MessageType::Metadata => {
                rpc_println!(frame_sink, "[device/v2] V2Timeouts={:?}", self.timeouts);
                // rpc_println!(frame_sink, "[device/v2] received V2/Metadata");
                let msg: Metadata = match postcard::from_bytes(payload) {
                    Ok(msg) => msg,
                    Err(e) => {
                        rpc_println!(
                            frame_sink,
                            "[device/v2] failed to parse payload (V2/Metadata): {:?}",
                            e
                        );
                        return ProtocolStatus::Continue;
                    }
                };
                self.recv_metadata(msg, frame_sink, timeouts);
            }
            MessageType::MetadataAckAck => {
                // rpc_println!(frame_sink, "[device/v2] received V2/MetadataAckAck");
                let msg: host::MetadataAckAck = match postcard::from_bytes(payload) {
                    Ok(msg) => msg,
                    Err(e) => {
                        rpc_println!(
                            frame_sink,
                            "[device/v2] failed to parse payload (V2/MetadataAckAck): {:?}",
                            e
                        );
                        return ProtocolStatus::Continue;
                    }
                };
                self.recv_metadata_ack_ack(msg, frame_sink);
            }
            MessageType::Chunk => {
                // rpc_println!(frame_sink, "[device/v2] received V2/Chunk");
                let msg: Chunk = match postcard::from_bytes(payload) {
                    Ok(msg) => msg,
                    Err(e) => {
                        rpc_println!(
                            frame_sink,
                            "[device/v2] failed to parse payload (V2/Chunk): {:?}",
                            e
                        );
                        return ProtocolStatus::Continue;
                    }
                };
                if !self.recv_chunk(msg, frame_sink, peripherals, inflate_buffer) {
                    // if this returns fails, CRC failed or other catastrophic error
                    return ProtocolStatus::Abend;
                }
            }
            MessageType::BootingAck => {
                // rpc_println!(frame_sink, "[device/v2] received V2/BootingAck");
                self.recv_booting_ack(frame_sink, peripherals);
            }
            otherwise => {
                rpc_println!(
                    frame_sink,
                    "[device/v2] unrecognized message type: {:?}, ignoring",
                    otherwise
                );
            }
        }
        ProtocolStatus::Continue
    }

    fn heartbeat(
        &mut self,
        frame_sink: &mut FrameSink,
        _timeouts: &mut Timeouts,
        peripherals: &Peripherals,
    ) -> ProtocolStatus {
        let send_once = core::mem::replace(&mut self.once, false);
        let heartbeat_elapsed = self.heartbeat.elapsed(&peripherals.SYSTMR);

        let should_send
            // A. first time message is being sent
            = send_once
            // B. retry due to no response - in the case of RequestChunk, we have a longer retry due
            //    to large message size
            || heartbeat_elapsed
            > if matches!(self.state, S::RequestChunk { .. }) {
            self.timeouts.try_resend_chunk
        } else {
            self.timeouts.try_resend
        }
            // C. retry due to failed send - retry after a while
            // XXX(mc): pretty sure this only happens if the buffer is full, so we're basically
            //          trying to make time to drain the buffer
            || (self.retry_buffer && heartbeat_elapsed > self.timeouts.buffer_retry);

        if should_send {
            let send_result = match &self.state {
                S::RequestMetadata => self.send_metadata_request(peripherals, frame_sink),
                S::AckMetadata(metadata) => {
                    self.send_metadata_ack(peripherals, frame_sink, *metadata)
                }
                S::RequestChunk {
                    which,
                    count: _,
                    loader: _,
                } => self.send_chunk_request(peripherals, frame_sink, *which),
                S::Boot { .. } => self.send_boot_msg(peripherals, frame_sink),
            };
            match send_result {
                Ok(true) => self.retry_buffer = false,
                Ok(false) => self.retry_buffer = true,
                Err(()) => return ProtocolStatus::Abend,
            }

            self.heartbeat = Instant::now(&peripherals.SYSTMR);
        }

        ProtocolStatus::Continue
    }
}

impl V2 {
    fn recv_metadata(
        &mut self,
        msg: host::Metadata,
        frame_sink: &mut FrameSink,
        timeouts: &mut Timeouts,
    ) {
        if !matches!(self.state, S::RequestMetadata) {
            rpc_println!(
                frame_sink,
                "[device/v2] received unexpected V2/Metadata in state: {:?}, ignoring.",
                self.state
            );
            return;
        }
        let ok = match msg.format_details {
            FormatDetails::Bin { load_address } => {
                if load_address >= 0x2000_0000 {
                    rpc_println!(frame_sink, "[device/v2] BIN file load address too high");
                    false
                } else if (load_address & 3) != 0 {
                    rpc_println!(
                        frame_sink,
                        "[device/v2] BIN file load address must be 4-byte aligned"
                    );
                    false
                } else {
                    true
                }
            }
            FormatDetails::Elf => true,
        };
        if ok {
            self.state = S::AckMetadata(msg);
            self.once = true;
            // override session timeout
            timeouts.override_session_timeout =
                Some(timeouts::TRY_RESEND_CHUNK.at_baud_8n1(self.baud) * 2)
        }
    }
    fn recv_metadata_ack_ack(&mut self, msg: host::MetadataAckAck, frame_sink: &mut FrameSink) {
        let S::AckMetadata(metadata) = &self.state else {
            rpc_println!(
                frame_sink,
                "[device/v2] received V2/MetadataAckAck in state: {:?}, ignoring.",
                self.state
            );
            return;
        };
        if !msg.is_ok {
            rpc_println!(
                frame_sink,
                "[device/v2] received V2/MetadataAckAck(ok=false), requesting metadata again"
            );
            self.state = S::RequestMetadata;
            return;
        }
        let chunk_count = (metadata.deflated_len as usize + CHUNK_SIZE - 1) / CHUNK_SIZE;
        let loader = match metadata.format_details {
            FormatDetails::Bin { load_address } => LoaderEnum::BinLoader(BinLoader::new(
                load_address.try_into().expect(
                    "cannot reach this point with load_address that is not representable as u32",
                ),
                metadata.clone(),
            )),
            FormatDetails::Elf => LoaderEnum::ElfLoader(ElfLoader::new(metadata.clone())),
        };
        self.state = S::RequestChunk {
            which: 0,
            count: chunk_count,
            loader,
        };
        self.once = true;
    }
    fn recv_chunk(
        &mut self,
        msg: Chunk,
        frame_sink: &mut FrameSink,
        peripherals: &Peripherals,
        inflate_buffer: &mut [u8],
    ) -> bool {
        let S::RequestChunk {
            which,
            count,
            loader,
        } = &mut self.state
        else {
            rpc_println!(
                frame_sink,
                "[device/v2] received unexpected V2/Chunk(which={}) in state: {:?}, ignoring.",
                msg.which,
                self.state
            );
            return true;
        };
        let finished = if *which == msg.which as usize {
            // received correct chunk, pass it to the loader

            let (a, b) = inflate_buffer.split_at_mut(0x2000);
            {
                let new_end = msg.bytes.len() + self.remainder;
                assert!(a.len() >= new_end);
                a[self.remainder..new_end].copy_from_slice(msg.bytes);
                self.remainder = new_end;
            }

            loop {
                // rpc_println!(frame_sink, "[device/v2] inflate on: {:02x?}", &a[..new_end]);
                let inflate_result = miniz_oxide::inflate::stream::inflate(
                    &mut self.inflate_state,
                    &a[..self.remainder],
                    b,
                    MZFlush::None,
                );
                // rpc_println!(
                //     frame_sink,
                //     "[device/v2] inflate to: {:02x?}",
                //     &b[..inflate_result.bytes_written]
                // );
                // rpc_println!(frame_sink, "[device/v2] inflate: {:?}", inflate_result);

                let mut done = false;
                match inflate_result.status {
                    Ok(stat) => match stat {
                        MZStatus::Ok => {
                            a.copy_within(inflate_result.bytes_consumed..self.remainder, 0);
                            self.remainder -= inflate_result.bytes_consumed;
                            if inflate_result.bytes_written == 0 {
                                break;
                            }
                        }
                        MZStatus::StreamEnd => {
                            self.remainder -= inflate_result.bytes_consumed;
                            done = true;
                        }
                        MZStatus::NeedDict => unreachable!(), // unused
                    },
                    Err(e) => match e {
                        MZError::Buf => {
                            // rpc_println!(frame_sink, "[device/v2] failed to make inflate progress");
                            assert_eq!(inflate_result.bytes_consumed, 0);
                            assert_eq!(inflate_result.bytes_written, 0);
                            break;
                        }
                        MZError::Data => {
                            rpc_println!(
                                frame_sink,
                                "[device/v2] MZError::Data probably indicates data corruption"
                            );
                            return false; // catastrophic
                        }
                        e => {
                            rpc_println!(
                                frame_sink,
                                "[device/v2] unexpected error while inflating: {:?}",
                                e
                            );
                            return false; // catastrophic
                        }
                    },
                }
                if let Err(e) = loader.receive_bytes(&b[..inflate_result.bytes_written]) {
                    rpc_println!(frame_sink, "[device/v2] unrecoverable load error: {}", e);
                    return false; // catastrophic
                }
                if done {
                    break;
                }
            }

            *which += 1;
            if *which == *count {
                // done
                rpc_println!(frame_sink, "[device/v2] processed last chunk");
                true // finished
            } else {
                false // not finished
            }
        } else {
            rpc_println!(
                frame_sink,
                "[device/v2] wrong chunk, expected {} got {}",
                *which,
                msg.which
            );
            false // not finished
        };
        self.once = true;
        if finished {
            let S::RequestChunk {
                which: _,
                count: _,
                loader,
            } = core::mem::replace(&mut self.state, S::RequestMetadata)
            else {
                unreachable!()
            };
            let booter = match Loader::finalize(loader, frame_sink, peripherals) {
                Ok(booter) => booter,
                Err(_e) => {
                    rpc_println!(frame_sink, "[device/v2] can't finalize, retrying");
                    return false;
                }
            };
            self.state = S::Boot { booter };
        }

        true
    }
    fn recv_booting_ack(&mut self, frame_sink: &mut FrameSink, peripherals: &Peripherals) {
        if !matches!(self.state, S::Boot { .. }) {
            rpc_println!(
                frame_sink,
                "[device/v2] received unexpected V2/BootingAck in state {:?}, ignoring.",
                self.state
            );
            return;
        };
        rpc_println!(frame_sink, "[device/v2] received V2/BootingAck, booting");
        let S::Boot { booter } = core::mem::replace(&mut self.state, S::RequestMetadata) else {
            unreachable!()
        };
        booter.enter(peripherals, frame_sink)
    }

    fn send_metadata_request(
        &mut self,
        _peripherals: &Peripherals,
        frame_sink: &mut FrameSink,
    ) -> Result<bool, ()> {
        match frame_sink.send(&device::MetadataReq {}) {
            Ok(()) => Ok(true),
            Err(SendError::Truncated) => Ok(false),
            Err(e) => {
                rpc_println!(
                    frame_sink,
                    "[device/v2] failed to send V2/MetadataReq: {}",
                    e
                );
                Err(())
            }
        }
    }
    fn send_metadata_ack(
        &mut self,
        _peripherals: &Peripherals,
        frame_sink: &mut FrameSink,
        metadata: Metadata,
    ) -> Result<bool, ()> {
        match frame_sink.send(&device::MetadataAck {
            chunk_size: CHUNK_SIZE as u32,
            metadata,
        }) {
            Ok(()) => Ok(true),
            Err(SendError::Truncated) => Ok(false),
            Err(e) => {
                rpc_println!(
                    frame_sink,
                    "[device/v2] failed to send V2/MetadataAck: {}",
                    e
                );
                Err(())
            }
        }
    }
    fn send_chunk_request(
        &mut self,
        _peripherals: &Peripherals,
        frame_sink: &mut FrameSink,
        which: usize,
    ) -> Result<bool, ()> {
        match frame_sink.send(&device::ChunkReq {
            which: which as u32,
        }) {
            Ok(()) => Ok(true),
            Err(SendError::Truncated) => Ok(false),
            Err(e) => {
                rpc_println!(frame_sink, "[device/v2] failed to send V2/ChunkReq: {}", e);
                Err(())
            }
        }
    }
    fn send_boot_msg(
        &mut self,
        _peripherals: &Peripherals,
        frame_sink: &mut FrameSink,
    ) -> Result<bool, ()> {
        match frame_sink.send(&device::Booting {}) {
            Ok(()) => Ok(true),
            Err(SendError::Truncated) => Ok(false),
            Err(e) => {
                rpc_println!(frame_sink, "[device/v2] failed to send V2/Booting: {}", e);
                Err(())
            }
        }
    }
}

#[derive(Debug)]
struct Booter {
    relocation: Relocation,
}
impl Booter {
    fn flat_binary(relocation: Relocation) -> Self {
        Self { relocation }
    }
    fn enter(self, peripherals: &Peripherals, frame_sink: &mut FrameSink) -> ! {
        unsafe {
            crate::stub::flat_binary::final_relocation(peripherals, frame_sink, self.relocation)
        }
    }
}

#[derive(Debug, Error)]
enum LoadError {
    #[error("CRC mismatch")]
    Crc,
}

#[enum_dispatch::enum_dispatch]
#[derive(Debug)]
enum LoaderEnum {
    BinLoader,
    ElfLoader,
}

#[enum_dispatch::enum_dispatch(LoaderEnum)]
trait Loader: Debug {
    fn receive_bytes(&mut self, bytes: &[u8]) -> Result<(), LoadError>;
    fn finalize(
        self,
        frame_sink: &mut FrameSink,
        peripherals: &Peripherals,
    ) -> Result<Booter, LoadError>;
}

#[derive(Debug)]
struct BinLoader {
    metadata: Metadata,

    relocation: Relocation,
    bytes_written: usize,
}
impl BinLoader {
    pub fn new(load_address: u32, metadata: Metadata) -> Self {
        let relocation = Relocation::calculate(
            load_address as usize,
            metadata.inflated_len as usize,
            unsafe { crate::stub::locate_end() }.addr(),
        );
        Self {
            metadata,
            relocation,
            bytes_written: 0,
        }
    }
}
impl Loader for BinLoader {
    fn receive_bytes(&mut self, bytes: &[u8]) -> Result<(), LoadError> {
        let ptr = unsafe {
            self.relocation
                .base_address_ptr
                .byte_offset(self.bytes_written as isize)
        };
        self.bytes_written += bytes.len();
        unsafe {
            self.relocation.write_bytes(ptr, bytes);
        }
        Ok(())
    }

    fn finalize(
        self,
        frame_sink: &mut FrameSink,
        peripherals: &Peripherals,
    ) -> Result<Booter, LoadError> {
        rpc_println!(frame_sink, "[device/v2] booter={self:?}");

        match unsafe {
            self.relocation.verify_integrity(
                self.metadata.inflated_crc,
                self.metadata.inflated_len as usize,
            )
        } {
            Integrity::Ok => {
                rpc_println!(frame_sink, "[device/v2] CRCs okay, running relocation stub");
                super::flush_to_fifo(frame_sink, &peripherals.UART1);
                Ok(Booter::flat_binary(self.relocation.clone()))
            }
            Integrity::CrcMismatch {
                expected,
                calculated,
            } => {
                rpc_println!(
                    frame_sink,
                    "[device/v2] CRC mismatch: expected {:#010x} calculated {:#010x}",
                    expected,
                    calculated
                );
                super::flush_to_fifo(frame_sink, &peripherals.UART1);
                Err(LoadError::Crc)
            }
        }
    }
}

#[derive(Debug)]
struct ElfLoader {
    #[allow(unused)]
    metadata: Metadata,
}
impl ElfLoader {
    pub fn new(metadata: Metadata) -> Self {
        Self { metadata }
    }
}
impl Loader for ElfLoader {
    fn receive_bytes(&mut self, _bytes: &[u8]) -> Result<(), LoadError> {
        todo!()
    }

    fn finalize(
        self,
        _frame_sink: &mut FrameSink,
        _peripherals: &Peripherals,
    ) -> Result<Booter, LoadError> {
        todo!()
    }
}
