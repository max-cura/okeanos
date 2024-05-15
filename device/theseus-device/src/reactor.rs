use core::cell::Cell;
use core::time::Duration;
use bcm2835_lpa::{Peripherals, SYSTMR};
use thiserror::Error;
use blinken::Blinken;
use rxbuf::FrameDataBuffer;
use theseus_common::cobs::FeedState;
use theseus_common::theseus::MessageTypeType;
use txbuf::TransmissionBuffer;
use crate::arm1176::__dsb;
use crate::{legacy, legacy_print_string, muart, timing};
use crate::reactor::handshake::Handshake;
use crate::reactor::txbuf::FrameSink;
use crate::timeouts;

pub mod txbuf;
pub mod handshake;
mod v1;
mod rxbuf;
pub mod blinken;

const INITIAL_BAUD_RATE : UartClock = UartClock::B115200;

const RECEIVE_BUFFER_SIZE : usize = 0x10000;
const TRANSMIT_BUFFER_SIZE : usize = 0x10000;
const COBS_ENCODE_BUFFER_SIZE : usize = 255;
const POSTCARD_ENCODE_BUFFER_SIZE : usize = 0x100;

const fn align_addr4(x: usize) -> usize {
    (x + 3) & !3
}

fn initialize() -> (Reactor, FrameSink) {
    let peripherals = unsafe { Peripherals::steal() };

    muart::uart1_init(
        &peripherals.GPIO,
        &peripherals.AUX,
        &peripherals.UART1,
        INITIAL_BAUD_RATE.to_divider(),
    );

    // legacy::fmt::boot_umsg!(uw, "[theseus-device]: performing timing test:");
    // legacy::fmt::boot_umsg!(uw, "[theseus-device]: 1000ms");
    // timing::delay_millis(&peripherals.SYSTMR, 1000);
    // legacy::fmt::boot_umsg!(uw, "[theseus-device]: done");

    let end_of_program = unsafe { core::ptr::addr_of!(super::stub::__symbol_exec_end__) } as usize;
    let buffer_space_start = align_addr4(end_of_program);

    let receive_buffer_addr = buffer_space_start;
    let transmit_buffer_addr = align_addr4(receive_buffer_addr + RECEIVE_BUFFER_SIZE);
    let cobs_buffer_addr = align_addr4(transmit_buffer_addr + TRANSMIT_BUFFER_SIZE);
    let postcard_encode_buffer_addr = align_addr4(cobs_buffer_addr + COBS_ENCODE_BUFFER_SIZE);
    let buffers_end_addr = align_addr4(postcard_encode_buffer_addr + POSTCARD_ENCODE_BUFFER_SIZE);

    let receive_buffer_ptr = receive_buffer_addr as *mut u8;
    let transmit_buffer_ptr = transmit_buffer_addr as *mut u8;
    let cobs_buffer_ptr = cobs_buffer_addr as *mut u8;
    let postcard_encode_buffer_ptr = postcard_encode_buffer_addr as *mut u8;
    let buffers_end_ptr = buffers_end_addr as *mut u8;

    let mut sbl = StationaryBufferLayout {
        receive_buffer: (receive_buffer_ptr, RECEIVE_BUFFER_SIZE),
        transmit_buffer: (transmit_buffer_ptr, TRANSMIT_BUFFER_SIZE),
        cobs_encode_buffer: cobs_buffer_ptr,
        postcard_encode_buffer: (postcard_encode_buffer_ptr, POSTCARD_ENCODE_BUFFER_SIZE),

        __unsafe_stationary_buffers_end__: buffers_end_ptr,
        __unsafe_memory_ends__: (512 * 1024 * 1024) as *mut u8,
    };

    let mut frame_sink = {
        let tx_buffer = TransmissionBuffer::new(sbl.transmit_buffer());
        let cobs_encoder = theseus_common::cobs::BufferedEncoder::with_buffer(
            sbl.cobs_encode_buffer(),
            0x55
        ).unwrap();
        let px_buffer = sbl.postcard_encode_buffer();

        FrameSink::new(tx_buffer, cobs_encoder, px_buffer)
    };

    legacy_print_string!(frame_sink, "[device]: reactor initialized");
    frame_sink._flush_to_fifo(&peripherals.UART1);
    muart::__flush_tx(&peripherals.UART1);

    (Reactor {
        peri: peripherals,
        layout: sbl,
        override_session_timeout: TakeCell::new(None)
    }, frame_sink)
}

pub struct StationaryBufferLayout {
    /// Buffer used to store reactor input (fixed size)
    receive_buffer: (*mut u8, usize),
    /// Buffer used to store reactor output (fixed size)
    /// TODO: calculate max required size of transmit buffer
    transmit_buffer: (*mut u8, usize),
    /// Buffer used for COBS encoding. Must be >=254 bytes. Only the first 254 bytes will be used.
    cobs_encode_buffer: *mut u8,
    /// Buffer used for Postcard marshaling.
    postcard_encode_buffer: (*mut u8, usize),

    /// The byte after the last byte of the fixed locations buffers.
    pub __unsafe_stationary_buffers_end__: *const  u8,
    /// The byte after the last byte of physical memory. Never dereference this.
    pub __unsafe_memory_ends__: *const u8,
}

impl StationaryBufferLayout {
    pub fn receive_buffer(&mut self) -> &'static mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.receive_buffer.0, self.receive_buffer.1) }
    }
    pub fn transmit_buffer(&mut self) -> &'static mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.transmit_buffer.0, self.transmit_buffer.1) }
    }
    pub fn cobs_encode_buffer(&mut self) -> &'static mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.cobs_encode_buffer, COBS_ENCODE_BUFFER_SIZE) }
    }
    pub fn postcard_encode_buffer(&mut self) -> &'static mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.postcard_encode_buffer.0, self.postcard_encode_buffer.1) }
    }
}

pub struct TakeCell<T> {
    inner: Cell<Option<T>>,
}

impl<T> TakeCell<T> {
    pub fn new(inner: T) -> Self {
        Self { inner: Cell::new(Some(inner)) }
    }
    pub fn with<U, F>(&self, f: F) -> Option<U>
        where F: FnOnce(&T) -> U,
    {
        let x = self.inner.replace(None);
        let result = x.as_ref().map(f);
        self.inner.set(x);
        result
    }
    pub fn set(&self, t: T) {
        self.inner.set(Some(t))
    }
}

pub struct Reactor {
    pub(crate) peri: Peripherals,
    layout: StationaryBufferLayout,

    override_session_timeout: TakeCell<Option<Duration>>,
}

#[derive(Debug, Error, Clone)]
#[non_exhaustive]
enum ReceiveError {
    #[error("incoming message overflowed receive buffer")]
    BufferOverflow,
    #[error("incoming message overran the FIFO")]
    FifoOverrun,
    #[error("encountered TEL={total_encoded_length} bytes without packet terminating")]
    FrameOverflow { total_encoded_length: usize },
    #[allow(dead_code)]
    #[error("legacy download encountered error and was unable to complete")]
    LegacyDownloadFailure,
    #[error("packet ends after RBC={received_byte_count} bytes instead of TEL={total_encoded_length} bytes")]
    FrameUnderflow { total_encoded_length: usize, received_byte_count: usize },
    #[error("packet CRC mismatch: declared={declared_crc:#010x} computed={computed_crc:#010x}")]
    CrcMismatch { declared_crc: u32, computed_crc: u32 },
    #[error("deserialization error: {e}")]
    Deserialize { e: postcard::Error },
    #[error("declared frame size too small: {len}")]
    FrameTooSmall { len: usize },
    #[error("protocol error")]
    Protocol,
}

// impl ReceiveError {
//     pub fn intish(&self) -> u8 {
//         match self {
//             ReceiveError::BufferOverflow => 1,
//             ReceiveError::FifoOverrun => 2,
//             ReceiveError::FrameOverflow { .. } => 3,
//             ReceiveError::LegacyDownloadFailure => 4,
//             ReceiveError::FrameUnderflow { .. } => 5,
//             ReceiveError::CrcMismatch { .. } => 6,
//             ReceiveError::Deserialize { .. } => 7,
//             ReceiveError::FrameTooSmall { .. } => 8,
//             ReceiveError::Protocol => 9,
//         }
//     }
// }

#[derive(Debug, Clone)]
#[repr(u8)]
enum ReceiveState {
    WaitingInitial = 1,
    WaitingNext = 2,

    // we're just going to reuse the legacy-mode code from the previous iteration because I don't
    // care enough to port it to the new architecture
    LegacyPutProgramInfo1 = 3,
    LegacyPutProgramInfo2 = 4,
    LegacyPutProgramInfo3 = 5,

    Preamble1 = 6,
    Preamble2 = 7,
    Preamble3 = 8,

    FrameSize0 = 9,
    FrameSize1(u8) = 10,
    FrameSize2(u8, u8) = 11,
    FrameSize3(u8, u8, u8) = 12,
    // {
    //     byte_no: usize,
    //     size: u32,
    // },
    CobsFrame {
        total_encoded_length: usize,
        received_byte_count: usize,
    } = 13,

    // Abcon
    Error {
        at_instant: timing::Instant,
        receive_error: Option<ReceiveError>
    } = 14,
}

impl ReceiveState {
    pub fn error(st: &SYSTMR, receive_error: ReceiveError) -> Self {
        Self::Error {
            at_instant: timing::Instant::now(st),
            receive_error: Some(receive_error),
        }
    }
    #[allow(unused)]
    // copied from core::mem::discriminant's docs
    fn discriminant(&self) -> u8 {
        // SAFETY: Because `Self` is marked `repr(u8)`, its layout is a `repr(C)` `union`
        // between `repr(C)` structs, each of which has the `u8` discriminant as its first
        // field, so we can read the discriminant without offsetting the pointer.
        unsafe { *<*const _>::from(self).cast::<u8>() }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Timeouts {
    error_recovery: Duration,
    byte_read: Duration,
    session_expires: Duration,
}
impl Timeouts {
    pub fn initial() -> Self {
        Self::new_8n1(INITIAL_BAUD_RATE.to_baud())
    }
    fn new_8n1(baud: u32) -> Timeouts {
        Self {
            error_recovery: timeouts::ERROR_RECOVERY.at_baud_8n1(baud),
            byte_read: timeouts::BYTE_READ.at_baud_8n1(baud),
            session_expires: timeouts::SESSION_EXPIRES.at_baud_8n1(baud),
        }
    }
}

pub struct GetProgInfoSender {
    last_sent: timing::Instant,
}

impl GetProgInfoSender {
    pub fn new(st: &SYSTMR) -> Self {
        Self {
            last_sent: timing::Instant::now(st),
        }
    }
    pub(crate) fn try_send_gpi_if_applicable(&mut self, st: &SYSTMR, fs: &mut FrameSink) -> bool {
        if self.last_sent.elapsed(st) >= timeouts::GET_PROG_INFO_INTERVAL && fs._buffer().is_empty() {
            static GET_PROG_INFO: &[u8] = &[0x22, 0x22, 0x11, 0x11];
            fs._buffer_mut().extend_from_slice(GET_PROG_INFO);
            self.last_sent = timing::Instant::now(st);
            true
        } else {
            false
        }
    }
}

#[derive(Debug)]
pub enum ProtocolResult {
    /// Continue parsing messages
    Continue,
    /// Abnormal condition, packet ignored
    Abcon,
    /// Abnormal condition, reset protocol state
    Abend,
    /// Self-explanatory. Should only be used by [`Handshake`]
    __SwitchProtocol(ProtocolEnum)
}

use v1::V1;

#[enum_dispatch::enum_dispatch]
#[derive(Debug)]
pub enum ProtocolEnum {
    Handshake,
    V1,
}

#[enum_dispatch::enum_dispatch(ProtocolEnum)]
pub trait Protocol {
    fn handle_packet(
        &mut self,

        mtt: MessageTypeType,
        msg_data: &[u8],

        reactor: &Reactor,
        tx_buffer: &mut FrameSink,
        timeouts: &mut Timeouts,
    ) -> ProtocolResult;

    fn heartbeat(
        &mut self,

        reactor: &Reactor,
        tx_buffer: &mut FrameSink,
        timeouts: &mut Timeouts,
    ) -> ProtocolResult;
}

fn reaction_loop(
    mut rz: Reactor,
    mut frame_sink: FrameSink,
) {
    let uart = &rz.peri.UART1;

    // tx_buffer contains frames that are already COBS-encoded and ready to send.
    let mut rx_buffer = FrameDataBuffer::new(rz.layout.receive_buffer());
    let mut cobs_decoder = theseus_common::cobs::LineDecoder::new();

    let blinken = Blinken::init(&rz.peri.GPIO);

    let mut timeouts = Timeouts::initial();
    let mut last_byte = timing::Instant::now(&rz.peri.SYSTMR);
    let mut last_packet = timing::Instant::now(&rz.peri.SYSTMR);
    let mut protocol = ProtocolEnum::Handshake(Handshake::new());
    let mut recv_state = ReceiveState::WaitingInitial;
    let mut gpi_sender = GetProgInfoSender::new(&rz.peri.SYSTMR);

    legacy_print_string!(frame_sink, "timeout configuration config: {timeouts:?}");

    loop {
        // Check if we should be transmitting anything, and if so fill the FIFO from the
        // transmission buffer if the transmission buffer is nonempty
        // Check if there's anything in the read FIFO, and if so put it through the receiver state
        // machine.
        // If we finish reading a frame:
        // - If we encounter an error with the received frame, wait for a gap interval, and rerun
        //   the current state's transmit method.
        // - If we decoded the frame successfully, then flush the outgoing messages and then process
        //   the messages in the transmission queue, then run the appropriate action

        // -- DEBUG --
        let mut tx_did_send = false;
        // -- END DEBUG --

        __dsb();
        let lsr = uart.lsr().read();
        __dsb();

        let can_read = lsr.data_ready().bit_is_set();
        // tx_empty() is a totally misleading name; really, it should really be named
        // 'tx_has_space_available'; note that this is LSR so destructive read.
        let can_write = lsr.tx_empty().bit_is_set();
        let is_overrun = lsr.rx_overrun().bit_is_set();

        if can_write {
            if let Some(b) = frame_sink._buffer_mut().shift_byte() {
                __dsb();
                uart.io().write(|w| {
                    unsafe { w.data().bits(b) }
                });
                __dsb();
                // legacy_print_string_blocking!(uart, "[{b:#04x}:{}]", b.as_ascii().unwrap_or(core::ascii::Char::DollarSign).as_str());
                tx_did_send = true;
            }
        }

        // blinken._4(&rz.peri.GPIO, tx_did_send);
        // blinken._3(&rz.peri.GPIO, can_read);
        blinken._27(&rz.peri.GPIO, tx_did_send);
        blinken._47(&rz.peri.GPIO, can_read);
        blinken._6(&rz.peri.GPIO, is_overrun);
        // blinken._8(&rz.peri.GPIO, (timing::__floating_time(&rz.peri.SYSTMR) / 1_000_000) % 2 == 0);

        // are we in read overrun? if so, it's a packet error
        if is_overrun {
            // consider this as a packet error
            recv_state = ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::FifoOverrun);
        }
        let byte = if can_read {
            __dsb();
            let byte = uart.io().read().data().bits();
            __dsb();

            Some(byte)
        } else {
            None
        };

        if matches!(recv_state, ReceiveState::WaitingInitial) {
            gpi_sender.try_send_gpi_if_applicable(&rz.peri.SYSTMR, &mut frame_sink);
        }
        protocol.heartbeat(&rz, &mut frame_sink, &mut timeouts);

        // blinken.set(&rz.peri.GPIO, recv_state.discriminant());

        recv_state = match (byte, recv_state) {
            (Some(0x44), ReceiveState::WaitingInitial) => ReceiveState::LegacyPutProgramInfo1,
            (Some(0x44), ReceiveState::LegacyPutProgramInfo1) => ReceiveState::LegacyPutProgramInfo2,
            (Some(0x33), ReceiveState::LegacyPutProgramInfo2) => ReceiveState::LegacyPutProgramInfo3,
            (Some(0x33), ReceiveState::LegacyPutProgramInfo3) => {
                // legacy::legacy_print_string_blocking!(uart, "[device]: received PUT_PROG_INFO");

                // handle legacy download
                legacy::perform_download(&rz.peri.UART1);

                // --- WARNING --- WARNING --- WARNING ---
                // if legacy::perform_download actually *returns*, then assume program state is
                // hopelessly corrupted and return so we can reinit

                return
            }

            (Some(0x55), ReceiveState::WaitingInitial) => {
                // legacy_print_string!(frame_sink, "[device]: caught 0x55 in WaitingInitial");
                ReceiveState::Preamble1
            }
            (Some(0x55), ReceiveState::WaitingNext) => {
                // legacy_print_string!(frame_sink, "[device]: caught 0x55 in WaitingNext");
                ReceiveState::Preamble1
            }
            (Some(0x55), ReceiveState::Preamble1) => ReceiveState::Preamble2,
            (Some(0x55), ReceiveState::Preamble2) => ReceiveState::Preamble3,
            (Some(0x5e), ReceiveState::Preamble3) => ReceiveState::FrameSize0,
            // Important! since it's 0x5555555e, we need to allow 0x55-slides in case of packet
            // droppage
            (Some(0x55), ReceiveState::Preamble3) => ReceiveState::Preamble3,

            // PROTOCOL REVISION
            (Some(x), ReceiveState::FrameSize0) => ReceiveState::FrameSize1(x),
            (Some(x), ReceiveState::FrameSize1(b0)) => ReceiveState::FrameSize2(b0, x),
            (Some(x), ReceiveState::FrameSize2(b0, b1)) => ReceiveState::FrameSize3(b0, b1, x),
            (Some(x), ReceiveState::FrameSize3(b0, b1, b2)) => {
                let len = theseus_common::theseus::len::decode_len(&[b0, b1, b2, x]);
                if len < 4 {
                    ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::FrameTooSmall { len: len as usize })
                } else {
                    cobs_decoder.reset();
                    ReceiveState::CobsFrame { total_encoded_length: len as usize, received_byte_count: 0 }
                }
            }

            (Some(x), ReceiveState::CobsFrame {
                total_encoded_length,
                received_byte_count,
            }) => {
                // very important note! received_byte_count is actually 1 lower than it should be!
                // so RBC>=TEL *actually* only triggers when the number of bytes received, *including this one*
                // is STRICTLY GREATER than the TEL.
                let res = if received_byte_count >= total_encoded_length {
                    ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::FrameOverflow { total_encoded_length })
                } else {
                    let byte = x ^ 0x55;
                    match cobs_decoder.feed(byte) {
                        // NOTE: this loop should never run more than once
                        FeedState::PacketFinished => 'packet: loop {
                            // legacy_print_string!(frame_sink, "[device]: received packet");
                            // blinken._5(&rz.peri.GPIO, true);

                            let packet = rx_buffer.finalize();

                            if total_encoded_length != (received_byte_count + 1) {
                                // frame shorter than declared
                                break 'packet ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::FrameUnderflow { total_encoded_length, received_byte_count: received_byte_count + 1 })
                            }
                            // legacy_print_string!(frame_sink, "[device]: packet length ok");
                            let crc_bytes: [u8; 4] = packet[packet.len()-4..].try_into().unwrap();
                            let declared_crc = u32::from_le_bytes(crc_bytes);
                            let data_frame_bytes = &packet[..packet.len()-4];
                            let computed_crc = crc32fast::hash(data_frame_bytes);
                            if declared_crc != computed_crc {
                                break 'packet ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::CrcMismatch { declared_crc, computed_crc })
                            }
                            // legacy_print_string!(frame_sink, "[device]: packet CRCs okay");
                            let (typ, data) = match postcard::take_from_bytes::<theseus_common::theseus::MessageTypeType>(data_frame_bytes) {
                                Ok(x) => x,
                                Err(e) => break 'packet ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::Deserialize { e }),
                            };
                            // legacy_print_string!(frame_sink, "[device]: packet is type {typ}");
                            match protocol.handle_packet(typ, data, &rz, &mut frame_sink, &mut timeouts) {
                                ProtocolResult::Continue => (),
                                ProtocolResult::Abcon => break 'packet ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::Protocol),
                                ProtocolResult::Abend => {
                                    // reset protocol
                                    protocol = ProtocolEnum::Handshake(Handshake::new());
                                    break 'packet ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::Protocol)
                                }
                                ProtocolResult::__SwitchProtocol(pe) => {
                                    protocol = pe;
                                }
                            }
                            // Note: it's very important to make sure our timeouts are up-to-date to
                            // prevent cutoff mechanisms from hitting at the wrong point.
                            last_packet = timing::Instant::now(&rz.peri.SYSTMR);
                            break 'packet ReceiveState::WaitingNext
                        }
                        FeedState::Byte(y) => {
                            match rx_buffer.push_byte(y) {
                                Ok(_) => ReceiveState::CobsFrame { total_encoded_length, received_byte_count: received_byte_count + 1 },
                                Err(e) => ReceiveState::error(&rz.peri.SYSTMR, e)
                            }
                        }
                        FeedState::Pass => {
                            ReceiveState::CobsFrame { total_encoded_length, received_byte_count: received_byte_count + 1 }
                        }
                    }
                };
                // if we're exiting the CobsFrame state, clear the receive buffer
                if !matches!(res, ReceiveState::CobsFrame {..}) {
                    rx_buffer.clear();
                }
                res
            }

            (_, ReceiveState::Error {
                at_instant,
                receive_error
            }) => {
                if let Some(receive_error) = receive_error {
                    // blinken.set(&rz.peri.GPIO, receive_error.intish());
                    legacy_print_string!(frame_sink, "[device]: receive error: {receive_error}");
                    // legacy::legacy_print_string_blocking!(uart, "[device]: receive error: {receive_error}");
                }
                // behaviour: wait for ERROR_RECOVERY_TIMEOUT
                if at_instant.elapsed(&rz.peri.SYSTMR) < timeouts.error_recovery {
                    // perpetuate error state
                    ReceiveState::Error {
                        at_instant,
                        receive_error: None
                    }
                } else {
                    ReceiveState::WaitingNext
                }
            }

            (_, state) => {
                let packet_elapsed = last_packet.elapsed(&rz.peri.SYSTMR);
                let byte_elapsed = last_byte.elapsed(&rz.peri.SYSTMR);

                let session_timeout = rz.override_session_timeout.with(|x| {
                    x.clone()
                }).flatten().unwrap_or(timeouts.session_expires);
                //
                // let session_timeout = match rz.override_session_timeout {
                //     None => &timeouts.session_expires,
                //     Some(d) => d,
                // };

                if packet_elapsed >= session_timeout //timeouts.session_expires
                    && !matches!(state, ReceiveState::WaitingInitial)
                    && byte_elapsed >= timeouts.byte_read
                {
                    last_packet = timing::Instant::now(&rz.peri.SYSTMR);
                    // blinken._5(&rz.peri.GPIO, false);
                    // Dump anything left in the transmission buffer onto the FIFOs and then reset
                    // the clock rate. Note that __uart1_set_clock we flush the FIFOS out, as well.
                    legacy_print_string!(frame_sink, "[device]: session expired after {packet_elapsed:?}, dumping.");
                    frame_sink._flush_to_fifo(&rz.peri.UART1);
                    // reset clock speed
                    muart::__uart1_set_clock(&rz.peri.UART1, INITIAL_BAUD_RATE.to_divider());
                    timeouts = Timeouts::new_8n1(INITIAL_BAUD_RATE.to_baud());
                    rz.override_session_timeout.set(None);
                    // Reset protocol to initial state.
                    protocol = ProtocolEnum::Handshake(Handshake::new());

                    ReceiveState::WaitingInitial
                } else if byte_elapsed >= timeouts.byte_read
                    && !matches!(state, ReceiveState::WaitingInitial)
                {
                    // legacy_print_string!(frame_sink, "[device]: packet read timeout in state {} after {byte_elapsed:?}, ignoring", state.discriminant());
                    last_byte = timing::Instant::now(&rz.peri.SYSTMR);
                    ReceiveState::WaitingNext
                } else {
                    state
                }
            }
        };

        // note: want to time last_byte from the end of the previous operation
        if byte.is_some() {
            last_byte = timing::Instant::now(&rz.peri.SYSTMR);
            // legacy::legacy_print_string_blocking!(uart, "[device]: got byte {} in state {recv_state:?}", byte.as_ref().copied().unwrap());
        }
    }
}


pub fn run() {
    let (rz, frame_sink) = initialize();
    reaction_loop(rz, frame_sink);
}

#[derive(Debug, Copy, Clone)]
pub enum UartClock {
    /// 115200Bd
    B115200 = 270,  // 115313

    // Conjectured:
    // B230400 = 134,  // 231481
    // B460800 = 66,   // 466417
    // B500000 = 61,   // 504032
    // B576000 = 53,   // 578703
    // B921600 = 32,   // 946969
    // B1152000 = 26,  // 1157407
    // B1500000 = 19,  // 1562500
    // B2000000 = 14,  // 2083333
    // B2500000 = 11,  // 2604166
    // B3000000 = 9, // 3125000
}

impl UartClock {
    pub fn to_baud(self) -> u32 {
        match self {
            UartClock::B115200 => 115200,
        }
    }
    pub fn to_divider(self) -> u16 {
        match self {
            UartClock::B115200 => 270,
        }
    }
}
