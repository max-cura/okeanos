use core::time::Duration;
use bcm2835_lpa::{GPIO, Peripherals, SYSTMR};
use embedded_io::ErrorType;
use thiserror::Error;
use theseus_common::cobs::FeedState;
use crate::arm1176::__dsb;
use crate::{legacy, muart, timing};

const INITIAL_BAUD_RATE : Baud = Baud::B115200;

const RECEIVE_BUFFER_SIZE : usize = 0x10000;
const TRANSMIT_BUFFER_SIZE : usize = 0x10000;
const COBS_ENCODE_BUFFER_SIZE : usize = 254;

const fn align_addr4(x: usize) -> usize {
    (x + 3) & !3
}

fn initialize() -> Reactor {
    let peripherals = unsafe { Peripherals::steal() };

    muart::uart1_init(
        &peripherals.GPIO,
        &peripherals.AUX,
        &peripherals.UART1,
        270, // muart::baud_to_clock_divider(INITIAL_BAUD_RATE as u32),
    );

    // TODO
    legacy::legacy_print_string!(&peripherals.UART1, "[device]: reactor initialized");
    // legacy::fmt::boot_umsg!(uw, "[theseus-device]: performing timing test:");
    // legacy::fmt::boot_umsg!(uw, "[theseus-device]: 1000ms");
    // timing::delay_millis(&peripherals.SYSTMR, 1000);
    // legacy::fmt::boot_umsg!(uw, "[theseus-device]: done");


    let end_of_program = unsafe { core::ptr::addr_of!(super::stub::__theseus_prog_end__) } as usize;
    let buffer_space_start = align_addr4(end_of_program);

    let receive_buffer_addr = buffer_space_start;
    let transmit_buffer_addr = align_addr4(receive_buffer_addr + RECEIVE_BUFFER_SIZE);
    let cobs_buffer_addr = align_addr4(transmit_buffer_addr + TRANSMIT_BUFFER_SIZE);
    let buffers_end_addr = align_addr4(cobs_buffer_addr + COBS_ENCODE_BUFFER_SIZE);

    let receive_buffer_ptr = receive_buffer_addr as *mut u8;
    let transmit_buffer_ptr = transmit_buffer_addr as *mut u8;
    let cobs_buffer_ptr = cobs_buffer_addr as *mut u8;
    let buffers_end_ptr = buffers_end_addr as *mut u8;

    let sbl = StationaryBufferLayout {
        receive_buffer: (receive_buffer_ptr, RECEIVE_BUFFER_SIZE),
        transmit_buffer: (transmit_buffer_ptr, TRANSMIT_BUFFER_SIZE),
        cobs_encode_buffer: cobs_buffer_ptr,

        __unsafe_stationary_buffers_end__: buffers_end_ptr,
        __unsafe_memory_ends__: (512 * 1024 * 1024) as *mut u8,
    };

    fn constitute_buf(p: (*mut u8, usize)) -> &'static mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(p.0, p.1) }
    }

    Reactor {
        peri: peripherals,
        in_buf: constitute_buf(sbl.receive_buffer),
        out_buf: constitute_buf(sbl.transmit_buffer),
        layout: sbl,
    }
}

pub struct StationaryBufferLayout {
    /// Buffer used to store reactor input (fixed size)
    receive_buffer: (*mut u8, usize),
    /// Buffer used to store reactor output (fixed size)
    /// TODO: calculate max required size of transmit buffer
    transmit_buffer: (*mut u8, usize),
    /// Buffer used for COBS encoding. Must be >=254 bytes. Only the first 254 bytes will be used.
    cobs_encode_buffer: *mut u8,


    /// The byte after the last byte of the fixed locations buffers.
    __unsafe_stationary_buffers_end__: *const  u8,
    /// The byte after the last byte of physical memory. Never dereference this.
    __unsafe_memory_ends__: *const u8,
}

pub struct Reactor {
    peri: Peripherals,
    in_buf: &'static mut [u8],
    out_buf: &'static mut [u8],
    layout: StationaryBufferLayout,
}

pub enum ReactorControl {
    Continue,
    Restart,
}

#[derive(Debug, Error, Copy, Clone)]
enum ReceiveError {
    #[error("incoming message overflowed receive buffer")]
    BufferOverflow,
    #[error("incoming message overran the FIFO")]
    FifoOverrun,
    #[error("frame has declared length zero")]
    ZeroLengthFrame,
    #[error("encountered TEL={total_encoded_length} bytes without packet terminating")]
    FrameOverflow { total_encoded_length: usize },
    #[error("legacy download encountered error and was unable to complete")]
    LegacyDownloadFailure,
}

#[derive(Debug, Copy, Clone)]
enum ReceiveState {
    Waiting,

    // we're just going to reuse the legacy-mode code from the previous iteration because I don't
    // care enough to port it to the new architecture
    LegacyPutProgramInfo1,
    LegacyPutProgramInfo2,
    LegacyPutProgramInfo3,

    Preamble1,
    Preamble2,
    Preamble3,
    FrameSize{
        byte_no: usize,
        size: u32,
    },
    CobsFrame {
        total_encoded_length: usize,
        received_byte_count: usize,
    },

    // Abcon
    Error {
        at_instant: timing::Instant,
        receive_error: Option<ReceiveError>
    },
}

impl ReceiveState {
    pub fn error(st: &SYSTMR, receive_error: ReceiveError) -> Self {
        Self::Error {
            at_instant: timing::Instant::now(st),
            receive_error: Some(receive_error),
        }
    }
}

pub mod timeouts {
    use core::time::Duration;

    #[derive(Debug, Copy, Clone)]
    pub struct RateRelativeTimeout {
        bytes: usize,
    }
    impl RateRelativeTimeout {
        pub const fn from_bytes(n: usize) -> Self {
            Self { bytes: n }
        }
        pub const fn at_baud_8n1(self, baud: u32) -> Duration {
            // at 8n1, we have flat 80% efficiency; then we have 1 byte/10 bits
            // so byte_rate = baud/10 B/s
            // so time = bytes / byte_rate
            // problem: byte_rate much higher than bytes; up to 3.125 MB/s
            // we don't have floats (yet), so we get a bit awkward, since we're in units of
            // microseconds; thus, we use fixed point on 10^6 and round up

            let byte_rate = baud / 10;

            let bytes_mega = (self.bytes * 1_000_000) as u32;
            let microseconds = (bytes_mega + byte_rate - 1) / byte_rate;

            Duration::from_micros(microseconds as u64)
        }
    }

    pub const ERROR_RECOVERY : RateRelativeTimeout = RateRelativeTimeout::from_bytes(12);

    pub const GET_PROG_INFO_INTERVAL : Duration = Duration::from_millis(300);
}

#[derive(Debug, Copy, Clone)]
pub struct Timeouts {
    error_recovery: Duration,
}
impl Timeouts {
    pub fn initial() -> Self {
        Self::new_8n1(INITIAL_BAUD_RATE as u32)
    }
    fn new_8n1(baud: u32) -> Timeouts {
        Self {
            error_recovery: timeouts::ERROR_RECOVERY.at_baud_8n1(baud)
        }
    }
}

pub struct GetProgInfoSender {
    should_send: bool,
    last_sent: timing::Instant,
}

impl GetProgInfoSender {
    pub fn new(st: &SYSTMR) -> Self {
        Self {
            should_send: true,
            last_sent: timing::Instant::now(st),
        }
    }
    pub fn set_sending(&mut self, should_send: bool) {
        self.should_send = should_send;
    }
    pub(crate) fn try_send_gpi_if_applicable(&mut self, st: &SYSTMR, tb: &mut TransmissionBuffer) -> bool {
        if self.last_sent.elapsed(st) >= timeouts::GET_PROG_INFO_INTERVAL {
            static GET_PROG_INFO: &[u8] = &[0x22, 0x22, 0x11, 0x11];
            tb.extend_from_slice(GET_PROG_INFO);
            self.last_sent = timing::Instant::now(st);
            true
        } else {
            false
        }
    }
}

struct Blinken;

impl Blinken {
    pub fn init(gpio: &GPIO) -> Self {
        __dsb();
        gpio.gpfsel2().modify(|_, w| w.fsel27().output());
        gpio.gpfsel4().modify(|_, w| w.fsel47().output());
        __dsb();
        Self
    }
    pub fn _27(&self, gpio: &GPIO, x: bool) {
        __dsb();
        if x {
            unsafe { gpio.gpset0().write_with_zero(|w| w.set27().set_bit()) };
        } else {
            unsafe { gpio.gpclr0().write_with_zero(|w| w.clr27().clear_bit_by_one()) };
        }
        __dsb();
    }
    pub fn _47(&self, gpio: &GPIO, x: bool) {
        __dsb();
        if x {
            unsafe { gpio.gpclr1().write_with_zero(|w| w.clr47().clear_bit_by_one()) };
        } else {
            unsafe { gpio.gpset1().write_with_zero(|w| w.set47().set_bit()) };
        }
        __dsb();
    }
}

fn reaction_loop(
    mut rz: Reactor,
) {
    let uart = &rz.peri.UART1;
    let mut recv_state = ReceiveState::Waiting;
    // tx_buffer contains frames that are already COBS-encoded and ready to send.
    let mut tx_buffer = TransmissionBuffer::new(rz.out_buf);
    #[allow(unused_mut)] // todo remove
    let mut _cobs_encoder = theseus_common::cobs::BufferedEncoder::with_buffer(
        unsafe { core::slice::from_raw_parts_mut(rz.layout.cobs_encode_buffer, 254) }
    ).unwrap();
    let mut rx_buffer = FrameDataBuffer::new(rz.in_buf);
    let mut cobs_decoder = theseus_common::cobs::LineDecoder::new();

    #[allow(unused_mut)] // todo remove
    let mut timeouts = Timeouts::initial();
    let mut gpi_sender = GetProgInfoSender::new(&rz.peri.SYSTMR);

    let blinken = Blinken::init(&rz.peri.GPIO);

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

        gpi_sender.try_send_gpi_if_applicable(&rz.peri.SYSTMR, &mut tx_buffer);

        __dsb();

        // -- DEBUG --
        let mut tx_did_send = false;
        // -- END DEBUG --

        let lsr = uart.lsr().read();
        // tx_empty() is a totally misleading name; really, it should really be named
        // 'tx_has_space_available'; note that this is LSR so destructive read.
        if lsr.tx_empty().bit_is_set() {
            if let Some(b) = tx_buffer.shift_byte() {
                uart.io().write(|w| unsafe { w.data().bits(b) });
                tx_did_send = true;
            }
        }

        blinken._27(&rz.peri.GPIO, tx_did_send);
        blinken._47(&rz.peri.GPIO, lsr.data_ready().bit_is_set());

        // are we in read overrun? if so, it's a packet erro
        if lsr.rx_overrun().bit_is_set() {
            // consider this as a packet error
            recv_state = ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::FifoOverrun);
        }
        if lsr.data_ready().bit_is_set() {
            let byte = uart.io().read().data().bits();

            __dsb();

            legacy::legacy_print_string!(&rz.peri.UART1, "state: {recv_state:?}");

            recv_state = match (byte, recv_state) {
                (0x44, ReceiveState::Waiting) => ReceiveState::LegacyPutProgramInfo1,
                (0x44, ReceiveState::LegacyPutProgramInfo1) => ReceiveState::LegacyPutProgramInfo2,
                (0x33, ReceiveState::LegacyPutProgramInfo2) => ReceiveState::LegacyPutProgramInfo3,
                (0x33, ReceiveState::LegacyPutProgramInfo3) => {
                    // handle legacy download
                    legacy::perform_download(&rz.peri.UART1);

                    // --- WARNING --- WARNING --- WARNING ---
                    // if legacy::perform_download actually *returns*, then assume program state is
                    // hopelessly corrupted and return so we can reinit
                    return
                }

                (0x55, ReceiveState::Waiting) => ReceiveState::Preamble1,
                (0x55, ReceiveState::Preamble1) => ReceiveState::Preamble2,
                (0x55, ReceiveState::Preamble2) => ReceiveState::Preamble3,
                (0x5e, ReceiveState::Preamble3) => ReceiveState::FrameSize { size: 0, byte_no: 0 },
                // Important! since it's 0x5555555e, we need to allow 0x55-slides in case of packet
                // droppage
                (0x55, ReceiveState::Preamble3) => ReceiveState::Preamble3,
                // Protocol: size is a LEB128-encoded u28, XOR'd with 0x55s on each byte.
                // Is the size of the entire COBS frame, including the sentinel 0x55 at the end.
                (x, ReceiveState::FrameSize { size, byte_no }) => {
                    // COBS artefact
                    let x = x ^ 0x55;
                    // LEB128 decode - we maintain the 0x55 as SENTINEL by disallowing zero-length
                    // packets
                    if x == 0x00 {
                        // error - zero length packet (not permitted)
                        ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::ZeroLengthFrame)
                    } else {
                        let new_size = size | (((x & 0x7f) as u32) << (7 * (byte_no as u32)));
                        if (x & 0x80) != 0 {
                            ReceiveState::FrameSize { size: new_size, byte_no: byte_no + 1 }
                        } else {
                            ReceiveState::CobsFrame { total_encoded_length: new_size as usize, received_byte_count: 0 }
                        }
                    }
                }

                (x, ReceiveState::CobsFrame {
                    total_encoded_length,
                    received_byte_count,
                }) => {
                    if received_byte_count >= total_encoded_length {
                        ReceiveState::error(&rz.peri.SYSTMR, ReceiveError::FrameOverflow { total_encoded_length })
                    } else {
                        let byte = x ^ 0x55;
                        match cobs_decoder.feed(byte) {
                            FeedState::PacketFinished => {
                                // okay done, time to process message
                                todo!("process message");
                                // ReceiveState::Waiting
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
                    }
                }

                (x, e @ ReceiveState::Error {
                    at_instant,
                    receive_error
                }) => {
                    if let Some(receive_error) = receive_error {
                        legacy::print_string!(tx_buffer, "[device]: receive error: {receive_error}");
                    }
                    // behaviour: wait for ERROR_RECOVERY_TIMEOUT
                    if at_instant.elapsed(&rz.peri.SYSTMR) < timeouts.error_recovery {
                        // perpetuate error state
                        ReceiveState::Error {
                            at_instant,
                            receive_error: None
                        }
                    } else {
                        ReceiveState::Waiting
                    }
                }

                (x, _) => {
                    ReceiveState::Waiting
                }
            };
        }
    }
}

/// Circular buffer with FIFO semantics. Overlong writes will be truncated.
/// Only use on the Pi Zero (see source comments).
///
/// Please note that the [`embedded_io::Write`] implementation for this is rather funky, since this
/// is a pure storage buffer, and we can't really block if something goes wrong since we can only
/// make progress in the reactor loop.
#[derive(Debug)]
struct TransmissionBuffer {
    underlying_storage: &'static mut [u8],
    // circle_end == circle_begin && circle_len == 0 -> buffer empty
    // circle_end == circle_begin && circle_len > 0 -> buffer full
    circle_begin: usize,
    circle_end: usize,
    circle_len: usize,
}

impl TransmissionBuffer {
    pub fn new(underlying_storage: &'static mut [u8]) -> Self {
        Self {
            underlying_storage,
            circle_begin: 0,
            circle_end: 0,
            circle_len: 0,
        }
    }

    pub fn shift_byte(&mut self) -> Option<u8> {
        (self.circle_len > 0).then(|| {
            let b = self.underlying_storage[self.circle_begin];
            self.circle_begin += 1;
            self.circle_len -= 1;
            b
        })
    }

    fn _push_byte_at_unchecked(&mut self, offset: usize, byte: u8) -> usize {
        self.underlying_storage[offset] = byte;
        if offset >= self.underlying_storage.len() { 0 } else { offset + 1}
    }

    fn _write_bytes_at_unchecked(&mut self, offset: usize, bytes: &[u8]) {
        let mut cursor = offset;
        for &byte in bytes.iter() {
            cursor = self._push_byte_at_unchecked(cursor, byte);
        }
    }

    pub fn push_byte(&mut self, byte: u8) -> bool {
        if self.circle_len != 0 && self.circle_begin == self.circle_end {
            // full
            false
        } else {
            self.circle_end = self._push_byte_at_unchecked(self.circle_end, byte);
            // self.underlying_storage[self.circle_end] = byte;
            // self.circle_end += 1;
            self.circle_len += 1;

            true
        }
    }

    fn wrapped_add(&self, a: usize, b: usize) -> (usize,bool) {
        // check: i+j<self.underlying_buffer.len()
        // ASSUME: i+j<usize::MAX since usize::MAX is more memory than we have on the Pi Zero
        let i = (a + b) % self.underlying_storage.len();
        (i, i < (a + b))
    }

    pub fn slide_to_slice(&mut self, target: &mut [u8]) -> bool {
        if self.circle_len < target.len() {
            // invalid: target is the wrong size, or we don't have enough bytes in our buffer to
            // service the request.
            false
        } else {
            let (end, end_wraps) = self.wrapped_add(self.circle_begin, self.circle_begin + target.len());
            if !end_wraps {
                target.copy_from_slice(&self.underlying_storage[self.circle_begin..end]);
            } else {
                target[..(self.underlying_storage.len() - self.circle_begin)]
                    .copy_from_slice(&self.underlying_storage[self.circle_begin..]);
                target[(self.underlying_storage.len() - self.circle_begin)..]
                    .copy_from_slice(&self.underlying_storage[..end]);
            }
            true
        }
    }

    pub fn remaining_space(&self) -> usize {
        self.underlying_storage.len() - self.circle_len
    }

    pub fn extend_from_slice(&mut self, src: &[u8]) -> bool {
        if src.len() > self.underlying_storage.len()
            || self.circle_len > (self.underlying_storage.len() - src.len())
        {
            return false
        }
        for &b in src.iter() {
            self.push_byte(b);
        }

        true
    }
    pub fn reserve(&mut self, n_bytes: usize) -> Option<usize> {
        (n_bytes > self.underlying_storage.len()
            || self.circle_len > (self.underlying_storage.len() - n_bytes))
            .then(|| {
                let v = self.circle_end;
                self.circle_end += 4;
                v
            })
    }

    pub fn checkpoint(&self) -> TransmissionBufferCheckpoint {
        TransmissionBufferCheckpoint {
            circle_begin: self.circle_begin,
            circle_end: self.circle_end,
            circle_len: self.circle_len,
        }
    }

    pub fn bytes_since_checkpoint(&self, cp: TransmissionBufferCheckpoint) -> usize {
        let cp_end = cp.circle_end;
        if self.circle_end < cp_end {
            (self.underlying_storage.len() - cp_end) + cp_end
        } else {
            self.circle_end - cp_end
        }
    }

    pub fn restore(&mut self, from: TransmissionBufferCheckpoint) {
        let TransmissionBufferCheckpoint {
            circle_begin, circle_end, circle_len
        } = from;
        self.circle_begin = circle_begin;
        self.circle_end = circle_end;
        self.circle_len = circle_len;
    }
}

#[derive(Debug, Copy, Clone)]
struct TransmissionBufferCheckpoint {
    circle_begin: usize,
    circle_end: usize,
    circle_len: usize,
}

impl LegacyPrintStringWriter for TransmissionBuffer {
    fn writer(&mut self) -> TxWriter {
        TxWriter::new(self)
    }
}
pub trait LegacyPrintStringWriter {
    fn writer(&mut self) -> TxWriter;
}

pub struct TxWriter<'a> {
    transmission_buffer: &'a mut TransmissionBuffer,
    checkpoint: TransmissionBufferCheckpoint,
    len_offset: usize,
    ok: bool
}
impl<'a> TxWriter<'a> {
    pub fn new(transmission_buffer: &'a mut TransmissionBuffer) -> Self {
        let checkpoint = transmission_buffer.checkpoint();
        static PRINT_STRING : &[u8; 4] = &[0xee, 0xee, 0xdd, 0xdd];
        let mut ok = transmission_buffer.extend_from_slice(PRINT_STRING);
        let len_offset = if ok {
            let len_offset = transmission_buffer.reserve(4);
            ok = len_offset.is_some();
            len_offset.unwrap()
        } else {
            0
        };
        Self { transmission_buffer, checkpoint, len_offset, ok }
    }
    pub fn finalize(self) -> bool {
        if !self.ok {
            self.transmission_buffer.restore(self.checkpoint)
        } else {
            self.transmission_buffer._write_bytes_at_unchecked(
                self.len_offset,
                &(self.transmission_buffer.bytes_since_checkpoint(self.checkpoint) as u32)
                    .to_le_bytes()
            );
        }
        self.ok
    }
}
impl<'a> core::fmt::Write for TxWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.ok {
            self.ok = self.transmission_buffer.extend_from_slice(s.as_bytes());
        }
        Ok(())
    }
}

impl ErrorType for TransmissionBuffer {
    type Error = embedded_io::ErrorKind;
}

/// Receive buffer with the data from a COBS frame.
struct FrameDataBuffer {
    underlying_storage: &'static mut [u8],
    cursor: usize,
}

impl FrameDataBuffer {
    pub fn new(underlying_storage: &'static mut [u8]) -> Self {
        Self { underlying_storage, cursor: 0 }
    }
    pub fn push_byte(&mut self, b: u8) -> Result<(), ReceiveError> {
        if self.cursor >= self.underlying_storage.len() {
            // that's not great
            return Err(ReceiveError::BufferOverflow);
        }
        self.underlying_storage[self.cursor] = b;
        self.cursor += 1;
        Ok(())
    }
    pub fn finalize(&mut self) -> &mut [u8] {
        let end = self.cursor;
        self.cursor = 0;
        &mut self.underlying_storage[..end]
    }
}


pub fn run() {
    let rz = initialize();
    reaction_loop(rz);
}

#[repr(u32)]
pub enum Baud {
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
