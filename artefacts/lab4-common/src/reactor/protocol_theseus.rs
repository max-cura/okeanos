use core::any::Any;
use core::time::Duration;
use theseus_common::theseus::{MessageTypeType, v1};
use crate::reactor::{Io, IoEncode, IoEncodeMarker, IoTimeoutsRelative, Logger, Protocol, ProtocolFlow, Reactor};
use crate::relocation::{Integrity, Relocation};
use crate::timing::Instant;

const CHUNK_SIZE : usize = 0x100;

pub(crate) mod timeouts {
    use crate::timeouts::RateRelativeTimeout;

    pub const ERROR_RECOVERY : RateRelativeTimeout = RateRelativeTimeout::from_bytes(12);
    pub const BYTE_READ : RateRelativeTimeout = RateRelativeTimeout::from_bytes(2);
    pub const SESSION_EXPIRES : RateRelativeTimeout = RateRelativeTimeout::from_bytes(12288 /* 0x3000 */);
    pub const SESSION_EXPIRES_LONG : RateRelativeTimeout = RateRelativeTimeout::from_bytes(super::CHUNK_SIZE * 16 * 2);

    pub const TRY_RESEND : RateRelativeTimeout = RateRelativeTimeout::from_bytes(0x50);
    // pub const TRY_RESEND: RateRelativeTimeout = RateRelativeTimeout::from_bytes(0x300);
    pub const TRY_RESEND_CHUNK : RateRelativeTimeout = RateRelativeTimeout::from_bytes(super::CHUNK_SIZE * 16);
    pub const BUFFER_RETRY: RateRelativeTimeout = RateRelativeTimeout::from_bytes(0x80);
}

#[derive(Debug, Copy, Clone)]
struct InfoState {
    pub load_at_addr: usize,

    pub compressed_len: usize,
    pub decompressed_len: usize,

    pub compressed_crc: u32,
    pub decompressed_crc: u32,
}

#[derive(Debug, Clone)]
struct LoadState {
    info: InfoState,
    chunk_no: usize,
    num_chunks: usize,
    hasher: crc32fast::Hasher,
    relocation: Relocation,
}

#[derive(Debug, Clone)]
enum State {
    RequestProgramInfo,
    RequestProgram { info: InfoState },
    RequestChunk { load: LoadState },
    Boot { relocation: Relocation }
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
    pub fn with_ir() -> Self {
        Self {
            try_resend: timeouts::TRY_RESEND.with_ir(),
            buffer_retry: timeouts::BUFFER_RETRY.with_ir(),
            try_resend_chunk: timeouts::TRY_RESEND_CHUNK.with_ir(),
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum SendResult {
    Ok,
    Failed,
}

impl SendResult {
    fn succeeded(success: bool) -> Self {
        match success {
            true => Self::Ok,
            false => Self::Failed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BootProtocol {
    heartbeat: Instant,
    state: State,
    // send one message without checking timeouts
    once: bool,
    // previous fs.send() failed, try buffering again in 128 byte-rate
    retry_buffer: bool,
    timeouts: V1Timeouts,
    final_relocation: unsafe fn(&mut Reactor, &mut dyn Io, Relocation) -> !,
}

impl BootProtocol {
    pub fn new(
        reactor: &mut Reactor,
        final_relocation: unsafe fn(&mut Reactor, &mut dyn Io, Relocation) -> !,
    ) -> Self {
        Self {
            heartbeat: Instant::now(&reactor.peri.SYSTMR),
            state: State::RequestProgramInfo,
            once: true,
            retry_buffer: false,
            timeouts: V1Timeouts::with_ir(),
            final_relocation
        }
    }

    pub fn default_driver_timeouts() -> IoTimeoutsRelative {
        IoTimeoutsRelative {
            error_recovery: timeouts::ERROR_RECOVERY,
            byte_read_timeout: timeouts::BYTE_READ,
            session_timeout: Some(timeouts::SESSION_EXPIRES),
            session_timeout_long: Some(timeouts::SESSION_EXPIRES_LONG),
        }
    }
}

impl Protocol for BootProtocol {
    fn protocol_handle(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut dyn Logger,
        msg: &[u8]
    ) -> ProtocolFlow {
        // pull out mtt and bytes
        let (typ, data) = match postcard::take_from_bytes::<theseus_common::theseus::MessageTypeType>(msg) {
            Ok(x) => x,
            Err(e) => {
                let _ = logger.writeln_fmt(reactor, format_args!("[device:v1]: failed to deserialize message type: {e}"));
                return ProtocolFlow::Abcon
            }
        };
        self.handle_packet(reactor, logger, typ, data)
    }

    fn protocol_heartbeat(
        &mut self,
        reactor: &mut Reactor,
        io: &mut dyn Io,
        logger: &mut dyn Logger,
    ) -> ProtocolFlow {
        self.heartbeat(reactor, io, logger)
    }
}

impl IoEncodeMarker for v1::device::RequestProgramInfo {}
impl IoEncodeMarker for v1::device::RequestProgram {}
impl IoEncodeMarker for v1::device::RequestChunk {}
impl IoEncodeMarker for v1::device::Booting {}

impl BootProtocol {
    fn handle_packet(
        &mut self,
        rz: &mut Reactor,
        logger: &mut dyn Logger,
        mtt: MessageTypeType,
        msg_data: &[u8],
    ) -> ProtocolFlow {
        match mtt {
            v1::MSG_PROGRAM_INFO => {
                let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: V1Timeouts={:?}", self.timeouts));
                let msg: v1::host::ProgramInfo = match postcard::from_bytes(&msg_data) {
                    Ok(x) => x,
                    Err(e) => {
                        let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: Deserialization error: {e} on bytes: {msg_data:?}, ignoring."));
                        return ProtocolFlow::Continue;
                    }
                };
                self.recv_program_info(rz, logger, &msg);
            }
            v1::MSG_PROGRAM_READY => {
                let msg: v1::host::ProgramReady = match postcard::from_bytes(&msg_data) {
                    Ok(x) => x,
                    Err(e) => {
                        let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: Deserialization error: {e} on bytes: {msg_data:?}, ignoring."));
                        return ProtocolFlow::Continue;
                    }
                };
                self.recv_program(rz, logger, &msg);
            }
            v1::MSG_CHUNK => {
                let msg: v1::host::Chunk = match postcard::from_bytes(&msg_data) {
                    Ok(x) => x,
                    Err(e) => {
                        let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: Deserialization error: {e} on bytes: {msg_data:?}, ignoring."));
                        return ProtocolFlow::Continue;
                    }
                };
                // logger.write_fmt(rz, format_args!("[device:v1]: Received chunk after {:?}.", self.heartbeat.elapsed(&rz.peri.SYSTMR));
                if !self.recv_chunk(rz, logger, &msg) {
                    // special bit: if this returns false, (integrity check) CRC failed or other catastrophic error
                    // TODO: full reboot to recover state?
                    return ProtocolFlow::Abend
                }
            }
            t => {
                let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: Unrecognized message type: {t}, ignoring."));
            }
        }

        ProtocolFlow::Continue
    }

    fn heartbeat(
        &mut self,
        rz: &mut Reactor,
        io: &mut dyn Io,
        logger: &mut dyn Logger,
    ) -> ProtocolFlow {
        let send_once = if self.once {
            self.once = false;
            true
        } else {
            false
        };

        let heartbeat_elapsed = self.heartbeat.elapsed(&rz.peri.SYSTMR);
        let should_send = send_once
            // special case! in RequestChunk, packets get long and we may need to wait a while, so
            // check if we're waiting for chunks before resending
            || heartbeat_elapsed > if matches!(self.state, State::RequestChunk {..}) {
            self.timeouts.try_resend_chunk
        } else {
            self.timeouts.try_resend
        }
            || (self.retry_buffer && heartbeat_elapsed > self.timeouts.buffer_retry);

        // if heartbeat_elapsed > Duration::from_millis(1000) {
        //     // logger.writeln_fmt(rz, format_args!("heartbeat"));
        //     self.heartbeat = Instant::now(&rz.peri.SYSTMR);
        // }

        if should_send {
            match { match &self.state {
                State::RequestProgramInfo => {
                    self.send_request_program_info(rz, io, logger)
                }
                State::RequestProgram { info } => {
                    self.send_request_program(rz, io, logger, info)
                }
                State::RequestChunk { load } => {
                    // logger.write_fmt(rz, format_args!("[device:v1]: requesting chunk after {heartbeat_elapsed:?}");
                    self.send_request_chunk(rz, io, logger, load)
                }
                State::Boot { relocation } => {
                    unsafe {
                        (self.final_relocation)(rz, io, relocation.clone())
                    }
                }
            } } {
                Ok(SendResult::Ok) => {
                    self.retry_buffer = false
                }
                Ok(SendResult::Failed) => {
                    self.retry_buffer = true
                }
                Err(_) => {
                    return ProtocolFlow::Abend;
                }
            }

            self.heartbeat = Instant::now(&rz.peri.SYSTMR);
        }

        ProtocolFlow::Continue
    }

    fn send_request_program_info(
        &self,
        rz: &mut Reactor,
        io: &mut dyn Io,
        logger: &mut dyn Logger,
    ) -> Result<SendResult, ()> {
        Ok(SendResult::succeeded(io.io_queue_message(rz, logger, &v1::device::RequestProgramInfo)))
    }

    fn send_request_program(
        &self,
        rz: &mut Reactor,
        io: &mut dyn Io,
        logger: &mut dyn Logger,
        info: &InfoState,
    ) -> Result<SendResult, ()> {
        Ok(SendResult::succeeded(io.io_queue_message(rz, logger, &v1::device::RequestProgram {
            chunk_size: CHUNK_SIZE as u32,
            verify_compressed_crc: info.compressed_crc,
            verify_decompressed_crc: info.decompressed_crc,
        })))
    }

    fn send_request_chunk(
        &self,
        rz: &mut Reactor,
        io: &mut dyn Io,
        logger: &mut dyn Logger,
        load: &LoadState,
    ) -> Result<SendResult, ()> {
        Ok(SendResult::succeeded(io.io_queue_message(rz, logger, &v1::device::RequestChunk {
            chunk_no: load.chunk_no as u32,
        })))
    }

    fn recv_program_info(
        &mut self,
        rz: &mut Reactor,
        logger: &mut dyn Logger,
        msg: &v1::host::ProgramInfo,
    ) {
        if !matches!(self.state, State::RequestProgramInfo) {
            let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: Received unexpected ProgramInfo in state: {:?}, ignoring.", self.state));
            return
        }
        self.state = State::RequestProgram {
            info: InfoState {
                load_at_addr: msg.load_at_addr as usize,
                compressed_len: msg.compressed_len as usize,
                decompressed_len: msg.compressed_len as usize,
                compressed_crc: msg.compressed_crc,
                decompressed_crc: msg.decompressed_crc,
            }
        };
        self.once = true;
    }

    fn recv_program(
        &mut self,
        rz: &mut Reactor,
        logger: &mut dyn Logger,
        _msg: &v1::host::ProgramReady,
    ) {
        if !matches!(self.state, State::RequestProgram {..}) {
            let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: Received unexpected ProgramReady in state: {:?}, ignoring.", self.state));
            return
        }
        let State::RequestProgram { info } = core::mem::replace(&mut self.state, State::RequestProgramInfo) else {
            let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: NOT IN REQUEST_PROGRAM"));
            return
        };
        self.state = State::RequestChunk {
            load: LoadState {
                info,
                chunk_no: 0,
                num_chunks: (info.compressed_len + CHUNK_SIZE - 1) / CHUNK_SIZE,
                hasher: crc32fast::Hasher::new(),
                relocation: Relocation::calculate(
                    info.load_at_addr,
                    info.decompressed_len,
                    rz.heap.highest()
                ),
            },
        };
        // logger.write_fmt(rz, format_args!("[device:v1]: {:?}", self.state);
        self.once = true;

        // IMPORTANT

        // --- VERY WEIRD ---
        rz.set_session_timeout = Some(true);
        // let io_any : &mut dyn Any = io.into();
        // let io = io_any.downcast_mut::<IrDriver>().unwrap();
        // io.use_long_session_timeout(true);
        // rz.override_session_timeout.set(Some(
        //     timeouts::TRY_RESEND_CHUNK.at_baud_8n1(self.baud)
        //         * 2
        // ))
    }

    fn recv_chunk(
        &mut self,
        rz: &mut Reactor,
        logger: &mut dyn Logger,
        msg: &v1::host::Chunk,
    ) -> bool {
        if !matches!(self.state, State::RequestChunk {..}) {
            let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: Received unexpected Chunk in state: {:?}, ignoring.", self.state));
            return true
        }
        let State::RequestChunk { load } = &self.state else {
            let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: NOT IN REQUEST_CHUNK (1)"));
            return true
        };
        // logger.write_fmt(rz, format_args!("[device:v1]: Received chunk {} (looking for {})", msg.chunk_no, load.chunk_no);
        if msg.chunk_no == load.chunk_no as u32 {
            let State::RequestChunk { load: LoadState { info, chunk_no, num_chunks, hasher, relocation }}
                = core::mem::replace(&mut self.state, State::RequestProgramInfo) else {
                let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: NOT IN REQUEST_CHUNK (1)"));
                return true
            };
            // hasher.update(msg.data);

            // write data
            let ptr = unsafe {
                relocation.base_address_ptr.offset(
                    (CHUNK_SIZE * chunk_no) as isize
                )
            };

            unsafe {
                relocation.write_bytes(ptr, msg.data);
            }

            // let mut buf = [0; CHUNK_SIZE * 3];
            // for (i, b) in msg.data.iter().enumerate() {
            //     let upper = (b & 0xf0) >> 4;
            //     buf[3*i+0] = if upper < 10 {
            //         b'0' + upper
            //     } else {
            //         b'a' + upper - 10
            //     };
            //     let lower = (b & 0x0f);
            //     buf[3*i+1] = if lower < 10 {
            //         b'0' + lower
            //     } else {
            //         b'a' + lower - 10
            //     };
            //     buf[3*i+2] = b' ';
            // }

            let new_chunk_no = chunk_no + 1;

            // logger.write_fmt(rz, format_args!("[device:v1]: wrote [{}] at {ptr:#?} ({new_chunk_no}/{num_chunks})",
            //     core::str::from_utf8(&buf[..msg.data.len()*3-1]).unwrap_or("<invalid utf-8>")
            // );
            // fs._flush_to_fifo(&rz.peri.UART1);

            if new_chunk_no == num_chunks {
                // relocate_jump
                // logger.write_fmt(rz, format_args!("[device:v1]: we should at this point jump to the relocation\
                //                        stub but this functionality has not been implemented yet.");
                // Check CRCs
                match unsafe { relocation.verify_integrity(info.decompressed_crc, info.decompressed_len) } {
                    Integrity::Ok => {
                        let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: CRCs okay, running relocation stub"));
                        rz.uart_buffer._flush_to_uart1_fifo(&rz.peri.UART1);
                    }
                    Integrity::CrcMismatch { expected, calculated } => {
                        let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: CRC mismatch: expected {expected:#010x} calculated {calculated:#010x}"));
                        rz.uart_buffer._flush_to_uart1_fifo(&rz.peri.UART1);
                        return false
                    }
                }

                unsafe {
                    self.state = State::Boot {
                        relocation
                    }
                }
            } else {
                self.state = State::RequestChunk {
                    load: LoadState {
                        info,
                        chunk_no: chunk_no + 1,
                        num_chunks,
                        hasher,
                        relocation,
                    }
                }
            }
        } else {
            let _ = logger.writeln_fmt(rz, format_args!("[device:v1]: Wrong chunk, expected {} got {}", load.chunk_no, msg.chunk_no));
        }
        self.once = true;

        true
    }
}

