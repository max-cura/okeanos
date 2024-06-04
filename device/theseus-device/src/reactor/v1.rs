use crate::reactor::txbuf::FrameSink;
use crate::reactor::{Protocol, ProtocolResult, Reactor, Timeouts};
use crate::stub::{Integrity, Relocation, __relocation_stub__, __relocation_stub_end__};
use crate::timing;
use core::time::Duration;
use theseus_common::theseus::{v1, MessageTypeType};

const CHUNK_SIZE: usize = 0x1000;

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
    // no booting state - we jump
}

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
pub struct V1 {
    heartbeat: timing::Instant,
    state: State,
    // send one message without checking timeouts
    once: bool,
    // previous fs.send() failed, try buffering again in 128 byte-rate
    retry_buffer: bool,
    baud: u32,
    timeouts: V1Timeouts,
}

impl V1 {
    pub fn new(rz: &Reactor, baud: u32) -> Self {
        Self {
            heartbeat: timing::Instant::now(&rz.peri.SYSTMR),
            state: State::RequestProgramInfo,
            once: true,
            retry_buffer: false,
            baud,
            timeouts: V1Timeouts::new_8n1(baud),
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

impl Protocol for V1 {
    fn handle_packet(
        &mut self,
        mtt: MessageTypeType,
        msg_data: &[u8],
        rz: &Reactor,
        fs: &mut FrameSink,
        _timeouts: &mut Timeouts,
    ) -> ProtocolResult {
        match mtt {
            v1::MSG_PROGRAM_INFO => {
                crate::print_rpc!(fs, "[device:v1]: V1Timeouts={:?}", self.timeouts);
                let msg: v1::host::ProgramInfo = match postcard::from_bytes(&msg_data) {
                    Ok(x) => x,
                    Err(e) => {
                        crate::print_rpc!(fs, "[device:v1]: Deserialization error: {e} on bytes: {msg_data:?}, ignoring.");
                        return ProtocolResult::Continue;
                    }
                };
                self.recv_program_info(rz, fs, &msg);
            }
            v1::MSG_PROGRAM_READY => {
                let msg: v1::host::ProgramReady = match postcard::from_bytes(&msg_data) {
                    Ok(x) => x,
                    Err(e) => {
                        crate::print_rpc!(fs, "[device:v1]: Deserialization error: {e} on bytes: {msg_data:?}, ignoring.");
                        return ProtocolResult::Continue;
                    }
                };
                self.recv_program(rz, fs, &msg);
            }
            v1::MSG_CHUNK => {
                let msg: v1::host::Chunk = match postcard::from_bytes(&msg_data) {
                    Ok(x) => x,
                    Err(e) => {
                        crate::print_rpc!(fs, "[device:v1]: Deserialization error: {e} on bytes: {msg_data:?}, ignoring.");
                        return ProtocolResult::Continue;
                    }
                };
                // crate::print_rpc!(fs, "[device:v1]: Received chunk after {:?}.", self.heartbeat.elapsed(&rz.peri.SYSTMR));
                if !self.recv_chunk(rz, fs, &msg) {
                    // special bit: if this returns false, CRC failed or other catastrophic error
                    // TODO: full reboot to recover state?
                    return ProtocolResult::Abend;
                }
            }
            t => {
                crate::print_rpc!(fs, "[device:v1]: Unrecognized message type: {t}, ignoring.");
            }
        }

        ProtocolResult::Continue
    }

    fn heartbeat(
        &mut self,
        rz: &Reactor,
        fs: &mut FrameSink,
        _timeouts: &mut Timeouts,
    ) -> ProtocolResult {
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

        if should_send {
            match {
                match &self.state {
                    State::RequestProgramInfo => self.send_request_program_info(rz, fs),
                    State::RequestProgram { info } => self.send_request_program(rz, fs, info),
                    State::RequestChunk { load } => {
                        // crate::print_rpc!(fs, "[device:v1]: requesting chunk after {heartbeat_elapsed:?}");
                        self.send_request_chunk(rz, fs, load)
                    }
                }
            } {
                Ok(SendResult::Ok) => self.retry_buffer = false,
                Ok(SendResult::Failed) => self.retry_buffer = true,
                Err(_) => {
                    return ProtocolResult::Abend;
                }
            }

            self.heartbeat = timing::Instant::now(&rz.peri.SYSTMR);
        }

        ProtocolResult::Continue
    }
}

impl V1 {
    fn send_request_program_info(
        &self,
        _rz: &Reactor,
        fs: &mut FrameSink,
    ) -> Result<SendResult, ()> {
        fs.send(&v1::device::RequestProgramInfo)
            .map(SendResult::succeeded)
            .map_err(|e| {
                crate::print_rpc!(fs, "[device:v1]: Serialization error: {e}, aborting.");
            })
    }

    fn send_request_program(
        &self,
        _rz: &Reactor,
        fs: &mut FrameSink,
        info: &InfoState,
    ) -> Result<SendResult, ()> {
        fs.send(&v1::device::RequestProgram {
            chunk_size: CHUNK_SIZE as u32,
            verify_compressed_crc: info.compressed_crc,
            verify_decompressed_crc: info.decompressed_crc,
        })
        .map(SendResult::succeeded)
        .map_err(|e| {
            crate::print_rpc!(fs, "[device:v1]: Serialization error: {e}, aborting.");
        })
    }

    fn send_request_chunk(
        &self,
        _rz: &Reactor,
        fs: &mut FrameSink,
        load: &LoadState,
    ) -> Result<SendResult, ()> {
        fs.send(&v1::device::RequestChunk {
            chunk_no: load.chunk_no as u32,
        })
        .map(SendResult::succeeded)
        .map_err(|e| {
            crate::print_rpc!(fs, "[device:v1]: Serialization error: {e}, aborting.");
        })
    }

    fn recv_program_info(
        &mut self,
        _rz: &Reactor,
        fs: &mut FrameSink,
        msg: &v1::host::ProgramInfo,
    ) {
        if !matches!(self.state, State::RequestProgramInfo) {
            crate::print_rpc!(
                fs,
                "[device:v1]: Received unexpected ProgramInfo in state: {:?}, ignoring.",
                self.state
            );
            return;
        }
        self.state = State::RequestProgram {
            info: InfoState {
                load_at_addr: msg.load_at_addr as usize,
                compressed_len: msg.compressed_len as usize,
                decompressed_len: msg.compressed_len as usize,
                compressed_crc: msg.compressed_crc,
                decompressed_crc: msg.decompressed_crc,
            },
        };
        self.once = true;
    }

    fn recv_program(&mut self, rz: &Reactor, fs: &mut FrameSink, _msg: &v1::host::ProgramReady) {
        if !matches!(self.state, State::RequestProgram { .. }) {
            crate::print_rpc!(
                fs,
                "[device:v1]: Received unexpected ProgramReady in state: {:?}, ignoring.",
                self.state
            );
            return;
        }
        let State::RequestProgram { info } =
            core::mem::replace(&mut self.state, State::RequestProgramInfo)
        else {
            crate::print_rpc!(fs, "[device:v1]: NOT IN REQUEST_PROGRAM");
            return;
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
                    &rz.layout,
                ),
            },
        };
        // crate::print_rpc!(fs, "[device:v1]: {:?}", self.state);
        self.once = true;

        // IMPORTANT
        rz.override_session_timeout
            .set(Some(timeouts::TRY_RESEND_CHUNK.at_baud_8n1(self.baud) * 2))
    }

    fn recv_chunk(&mut self, rz: &Reactor, fs: &mut FrameSink, msg: &v1::host::Chunk) -> bool {
        if !matches!(self.state, State::RequestChunk { .. }) {
            crate::print_rpc!(
                fs,
                "[device:v1]: Received unexpected Chunk in state: {:?}, ignoring.",
                self.state
            );
            return true;
        }
        let State::RequestChunk { load } = &self.state else {
            crate::print_rpc!(fs, "[device:v1]: NOT IN REQUEST_CHUNK (1)");
            return true;
        };
        // crate::print_rpc!(fs, "[device:v1]: Received chunk {} (looking for {})", msg.chunk_no, load.chunk_no);
        if msg.chunk_no == load.chunk_no as u32 {
            let State::RequestChunk {
                load:
                    LoadState {
                        info,
                        chunk_no,
                        num_chunks,
                        hasher,
                        relocation,
                    },
            } = core::mem::replace(&mut self.state, State::RequestProgramInfo)
            else {
                crate::print_rpc!(fs, "[device:v1]: NOT IN REQUEST_CHUNK (1)");
                return true;
            };
            // hasher.update(msg.data);

            // write data
            let ptr = unsafe {
                relocation
                    .base_address_ptr
                    .offset((CHUNK_SIZE * chunk_no) as isize)
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

            // crate::print_rpc!(fs, "[device:v1]: wrote [{}] at {ptr:#?} ({new_chunk_no}/{num_chunks})",
            //     core::str::from_utf8(&buf[..msg.data.len()*3-1]).unwrap_or("<invalid utf-8>")
            // );
            // fs._flush_to_fifo(&rz.peri.UART1);

            if new_chunk_no == num_chunks {
                // relocate_jump
                // crate::print_rpc!(fs, "[device:v1]: we should at this point jump to the relocation\
                //                        stub but this functionality has not been implemented yet.");
                // Check CRCs
                match unsafe {
                    relocation.verify_integrity(
                        rz,
                        fs,
                        info.decompressed_crc,
                        info.decompressed_len,
                    )
                } {
                    Integrity::Ok => {
                        crate::print_rpc!(fs, "[device:v1]: CRCs okay, running relocation stub");
                        fs._flush_to_fifo(&rz.peri.UART1);
                    }
                    Integrity::CrcMismatch {
                        expected,
                        calculated,
                    } => {
                        crate::print_rpc!(fs, "[device:v1]: CRC mismatch: expected {expected:#010x} calculated {calculated:#010x}");
                        fs._flush_to_fifo(&rz.peri.UART1);
                        return false;
                    }
                }

                unsafe { final_relocation(rz, fs, relocation) }
            } else {
                self.state = State::RequestChunk {
                    load: LoadState {
                        info,
                        chunk_no: chunk_no + 1,
                        num_chunks,
                        hasher,
                        relocation,
                    },
                }
            }
        } else {
            crate::print_rpc!(
                fs,
                "[device:v1]: Wrong chunk, expected {} got {}",
                load.chunk_no,
                msg.chunk_no
            );
        }
        self.once = true;

        true
    }
}

unsafe fn final_relocation(rz: &Reactor, fs: &mut FrameSink, relocation: Relocation) -> ! {
    let blinken = super::Blinken::init(&rz.peri.GPIO);
    blinken.set(&rz.peri.GPIO, 0);
    // blinken._5(&rz.peri.GPIO, false);

    let stub_dst = relocation.stub_entry;
    let kernel_dst = relocation.base_address_ptr;
    let kernel_src = relocation.side_buffer_ptr;
    let kernel_copy_len = relocation.relocate_first_n_bytes;
    let kernel_entry = relocation.base_address_ptr;

    let stub_begin = core::ptr::addr_of!(__relocation_stub__);
    let stub_end = core::ptr::addr_of!(__relocation_stub_end__);

    let stub_len = stub_end.byte_offset_from(stub_begin) as usize;

    crate::legacy_print_string_blocking!(
        &rz.peri.UART1,
        "[device:v1]: relocation_stub parameters:"
    );
    crate::legacy_print_string_blocking!(&rz.peri.UART1, "\tstub destination={stub_dst:#?}");
    crate::legacy_print_string_blocking!(&rz.peri.UART1, "\tstub code={stub_begin:#?}");
    crate::legacy_print_string_blocking!(&rz.peri.UART1, "\tstub length={stub_len:#?}");
    crate::legacy_print_string_blocking!(&rz.peri.UART1, "\tcopy to={kernel_dst:#?}");
    crate::legacy_print_string_blocking!(&rz.peri.UART1, "\tcopy from={kernel_src:#?}");
    crate::legacy_print_string_blocking!(&rz.peri.UART1, "\tcopy bytes={kernel_copy_len}");
    crate::legacy_print_string_blocking!(&rz.peri.UART1, "\tentry={kernel_entry:#?}");

    core::ptr::copy(stub_begin as *const u8, stub_dst, stub_len);

    crate::legacy_print_string_blocking!(
        &rz.peri.UART1,
        "[device:v1]: Loaded relocation-stub, jumping"
    );

    fs._flush_to_fifo(&rz.peri.UART1);

    let _ = fs.send(&v1::device::Booting);

    fs._flush_to_fifo(&rz.peri.UART1);
    crate::muart::__flush_tx(&rz.peri.UART1);

    core::arch::asm!(
        "bx {t0}",
        in("r0") kernel_dst,
        in("r1") kernel_src,
        in("r2") kernel_copy_len,
        in("r3") kernel_entry,
        t0 = in(reg) stub_dst,
        options(noreturn),
    )
}
