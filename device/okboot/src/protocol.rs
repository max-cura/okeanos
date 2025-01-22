mod handshake;
mod v2;

use crate::arch::arm1176::__dsb;
use crate::arch::mini_uart::mini_uart1_flush_tx;
use crate::arch::timing;
use crate::arch::timing::Instant;
use crate::buf::{FrameSink, ReceiveBuffer, TransmitBuffer};
use crate::{arch, legacy_print_string, legacy_print_string_blocking, stub, timeouts};
use bcm2835_lpa::{Peripherals, SYSTMR, UART1};
use core::arch::asm;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::time::Duration;
use okboot_common::frame::{BufferedEncoder, FrameError, FrameHeader, FrameLayer, FrameOutput};
use okboot_common::{COBS_XOR, INITIAL_BAUD_RATE};
use thiserror::Error;

use handshake::Handshake;
use v2::V2;

const COBS_ENCODE_BUFFER_SIZE: usize = 255;

#[enum_dispatch::enum_dispatch]
#[derive(Debug)]
pub enum ProtocolEnum {
    Handshake,
    V2,
}
impl Default for ProtocolEnum {
    fn default() -> Self {
        Self::Handshake(Handshake::default())
    }
}

#[enum_dispatch::enum_dispatch(ProtocolEnum)]
pub trait Protocol {
    fn handle_packet(
        &mut self,
        frame_header: FrameHeader,
        payload: &[u8],
        frame_sink: &mut FrameSink,
        timeouts: &mut Timeouts,
        peripherals: &Peripherals,
        inflate_buffer: &mut [u8],
    ) -> ProtocolStatus;

    fn heartbeat(
        &mut self,
        frame_sink: &mut FrameSink,
        timeouts: &mut Timeouts,
        peripherals: &Peripherals,
    ) -> ProtocolStatus;
}

pub fn flush_to_fifo(sink: &mut FrameSink, uart: &UART1) {
    __dsb();
    while let Some(b) = sink.buffer_mut().shift_byte() {
        while uart.stat().read().tx_ready().bit_is_clear() {}
        uart.io().write(|w| unsafe { w.data().bits(b) })
    }
    __dsb();
}

struct GetProgInfoSender {
    last_sent_at: Instant,
}
impl GetProgInfoSender {
    pub fn new(st: &SYSTMR) -> Self {
        Self {
            last_sent_at: Instant::now(st),
        }
    }
    pub(crate) fn tick(&mut self, st: &SYSTMR, fs: &mut FrameSink) -> bool {
        if self.last_sent_at.elapsed(st) >= timeouts::GET_PROG_INFO_INTERVAL
            && fs.buffer().is_empty()
        {
            static GET_PROG_INFO: &[u8] = &[0x22, 0x22, 0x11, 0x11];
            fs.buffer_mut().extend_from_slice(GET_PROG_INFO);
            self.last_sent_at = timing::Instant::now(st);
            true
        } else {
            false
        }
    }
}

pub fn run(peripherals: &Peripherals) {
    let mut sp: u32;
    unsafe {
        asm!(
        "mov {t}, sp",
        "wfe",
        t = out(reg) sp
        );
    }
    legacy_print_string_blocking!(&peripherals.UART1, "<SP={sp:08x}>");
    const _: () = {
        assert!(
            INITIAL_BAUD_RATE == 115200,
            "B115200_DIVIDER adjustment required"
        );
    };
    const B115200_DIVIDER: u16 = 270;
    // arch::mini_uart::muart1_init(
    //     &peripherals.GPIO,
    //     &peripherals.AUX,
    //     &peripherals.UART1,
    //     B115200_DIVIDER,
    // );

    let uart = &peripherals.UART1;

    // let end_of_program = unsafe { stub::locate_end() };
    // let buffer_space_start = (end_of_program.addr() + 3) & !3;
    // legacy_print_string_blocking!(
    //     &peripherals.UART1,
    //     "EOP={end_of_program:#?} BufS={buffer_space_start:#?}"
    // );
    // {
    //     let mut tmp = crate::legacy::fmt::UartWrite::new((&peripherals.UART1));
    //     let bub = unsafe { &mut *crate::legacy::fmt::BOOT_UMSG_BUF.0.get() };
    //     bub.clear();
    //     let _ = bub.write_fmt(format_args!(
    //         "EOP={end_of_program:#?} BufS={buffer_space_start:#?}",
    //         end_of_program = end_of_program,
    //         buffer_space_start = buffer_space_start
    //     ));
    //     let _ = tmp.write_str(bub.as_str());
    //     crate::legacy::uart1::uart1_flush_tx((&peripherals.UART1))
    // }
    mini_uart1_flush_tx(&peripherals.UART1);
    // let mut arena = unsafe {
    //     BufferArena::new(
    //         buffer_space_start,
    //         RawBufferConfig {
    //             receive: 0x10000,
    //             transmit: 0x10000,
    //             staging: 0x10000,
    //             inflate: 0x10000,
    //         },
    //     )
    // };
    // legacy_print_string_blocking!(&peripherals.UART1, "POINT 0\n");
    mini_uart1_flush_tx(&peripherals.UART1);

    let AllocatedBuffers {
        receive_buffer,
        transmit_buffer,
        staging_buffer,
        cobs_encode_buffer,
        inflate_buffer,
        // } = unsafe { arena.take().unwrap() };
    } = unsafe { STATIC_BUFFERS.get() };

    // legacy_print_string_blocking!(&peripherals.UART1, "POINT 1\n");
    let mut frame_sink = {
        let tx_buffer = TransmitBuffer::new(transmit_buffer);
        let cobs_encoder = BufferedEncoder::with_buffer_xor(cobs_encode_buffer, COBS_XOR);
        let px_buffer = staging_buffer;
        FrameSink::new(tx_buffer, cobs_encoder, px_buffer)
    };
    // legacy_print_string_blocking!(&peripherals.UART1, "POINT 2\n");

    legacy_print_string!(&mut frame_sink, "[device]: starting state machine\n");
    flush_to_fifo(&mut frame_sink, uart);
    mini_uart1_flush_tx(uart);

    enum ReceiveState {
        Waiting {
            initial: bool,
        },
        Error {
            at_instant: timing::Instant,
            receive_error: Option<ReceiveError>,
        },
    }
    impl ReceiveState {
        pub fn error(systmr: &SYSTMR, error: ReceiveError) -> Self {
            Self::Error {
                at_instant: Instant::now(systmr),
                receive_error: Some(error),
            }
        }
    }

    let mut rx_buffer = ReceiveBuffer::new(receive_buffer);
    let mut decoder = FrameLayer::new(COBS_XOR);

    let mut timeouts = Timeouts::new_8n1(INITIAL_BAUD_RATE);
    let mut last_byte_received = Instant::now(&peripherals.SYSTMR);
    let mut last_packet_received = Instant::now(&peripherals.SYSTMR);
    let mut recv_state = ReceiveState::Waiting { initial: true };
    let mut gpi_sender = GetProgInfoSender::new(&peripherals.SYSTMR);
    let mut protocol = ProtocolEnum::Handshake(Handshake::default());
    let mut frame_header = None;

    legacy_print_string!(
        &mut frame_sink,
        "[device]: timeout configuration={timeouts:?}\n"
    );

    let mut last_uart_recv = Instant::now(&peripherals.SYSTMR);

    // core::hint::black_box(rx_buffer);
    // core::hint::black_box(decoder);
    // core::hint::black_box(last_byte_received);
    // core::hint::black_box(last_packet_received);
    // core::hint::black_box(recv_state);
    // core::hint::black_box(gpi_sender);
    // core::hint::black_box(protocol);
    // core::hint::black_box(frame_header);
    // core::hint::black_box(last_uart_recv);

    loop {
        // -- debug --
        let mut tx_did_send = false;
        // -- end debug --

        __dsb();
        let lsr = uart.lsr().read();

        let data_available = lsr.data_ready().bit_is_set();
        let can_write = lsr.tx_empty().bit_is_set();
        let is_overrun = lsr.rx_overrun().bit_is_set();

        if can_write {
            if let Some(b) = frame_sink.buffer_mut().shift_byte() {
                uart.io().write(|w| unsafe { w.data().bits(b) });
                tx_did_send = true;
            }
        }
        __dsb();

        if is_overrun {
            recv_state = ReceiveState::error(&peripherals.SYSTMR, ReceiveError::FifoOverrun);
        }
        let byte = if data_available {
            __dsb();
            let byte = uart.io().read().data().bits();
            __dsb();

            Some(byte)
        } else {
            None
        };

        if matches!(recv_state, ReceiveState::Waiting { initial: true }) {
            gpi_sender.tick(&peripherals.SYSTMR, &mut frame_sink);
        }

        protocol.heartbeat(&mut frame_sink, &mut timeouts, peripherals);

        recv_state = match (byte, recv_state) {
            (Some(b), ReceiveState::Waiting { initial: _ }) => {
                // last_uart_recv = Instant::now(&peripherals.SYSTMR);
                // legacy_print_string!(
                //     &mut frame_sink,
                //     "{:?}",
                //     last_uart_recv.elapsed(&peripherals.SYSTMR)
                // );
                let r = match decoder.feed(b) {
                    Ok(o) => {
                        match o {
                            FrameOutput::Skip => ReceiveState::Waiting { initial: false },
                            FrameOutput::Header(hdr) => {
                                frame_header = Some(hdr);
                                ReceiveState::Waiting { initial: false }
                            }
                            FrameOutput::Payload(p) => match rx_buffer.push_u8(p) {
                                Ok(_) => ReceiveState::Waiting { initial: false },
                                Err(e) => ReceiveState::error(&peripherals.SYSTMR, e),
                            },
                            FrameOutput::Finished => {
                                let frame_header = frame_header.take().unwrap();
                                let payload = rx_buffer.finalize();
                                decoder.reset();

                                let res = match protocol.handle_packet(
                                    frame_header,
                                    payload,
                                    &mut frame_sink,
                                    &mut timeouts,
                                    peripherals,
                                    inflate_buffer,
                                ) {
                                    ProtocolStatus::Continue => None,
                                    ProtocolStatus::Abcon => {
                                        // TODO
                                        Some(ReceiveState::error(
                                            &peripherals.SYSTMR,
                                            ReceiveError::Protocol,
                                        ))
                                    }
                                    ProtocolStatus::Abend => {
                                        protocol = ProtocolEnum::Handshake(Handshake::default());
                                        // TODO
                                        Some(ReceiveState::error(
                                            &peripherals.SYSTMR,
                                            ReceiveError::Protocol,
                                        ))
                                    }
                                    ProtocolStatus::Switch(pe) => {
                                        protocol = pe;
                                        None
                                    }
                                };
                                rx_buffer.clear();

                                last_packet_received = Instant::now(&peripherals.SYSTMR);
                                res.unwrap_or(ReceiveState::Waiting { initial: false })
                            }
                            FrameOutput::Legacy => {
                                decoder.reset();
                                // received PUT_PROG_INFO
                                // handle legacy download

                                crate::legacy::perform_download(&peripherals.UART1);

                                // if legacy::perform_download actually returns, then assume program
                                // state is hopelessly corrupted and return so we can reinit.
                                return;
                            }
                            FrameOutput::LegacyPrintStringByte(_, _) => {
                                decoder.reset();
                                legacy_print_string!(
                                    &mut frame_sink,
                                    "[device] received legacy PRINT_STRING from"
                                );
                                ReceiveState::error(&peripherals.SYSTMR, ReceiveError::Protocol)
                            }
                        }
                    }
                    Err(e) => {
                        decoder.reset();
                        ReceiveState::error(&peripherals.SYSTMR, ReceiveError::Decode(e))
                    }
                };
                r
            }

            // CASE: Receive error. Print error message ONE time, and then wait for error recovery
            //       timeout to elapse before returning to normal protocol execution.
            (
                _,
                ReceiveState::Error {
                    at_instant,
                    receive_error,
                },
            ) => {
                if let Some(receive_error) = receive_error {
                    legacy_print_string!(
                        &mut frame_sink,
                        "[device]: receive error: {receive_error}"
                    );
                }
                if at_instant.elapsed(&peripherals.SYSTMR) < timeouts.error_recovery {
                    ReceiveState::Error {
                        at_instant,
                        receive_error: None,
                    }
                } else {
                    ReceiveState::Waiting { initial: false }
                }
            }

            // CASE: Did not receive a byte that was a coherent part of the protocol. Specifically,
            //       either did not receive a byte, OR we're in the initial preamble state and the
            //       wrong byte was received.
            (_, state) => {
                let packet_elapsed = last_packet_received.elapsed(&peripherals.SYSTMR);
                let byte_elapsed = last_byte_received.elapsed(&peripherals.SYSTMR);

                let session_timeout = timeouts
                    .override_session_timeout
                    .clone()
                    .unwrap_or(timeouts.session_expires);

                if packet_elapsed >= session_timeout
                    && !matches!(state, ReceiveState::Waiting { initial: true })
                    && byte_elapsed >= timeouts.byte_read
                {
                    last_packet_received = timing::Instant::now(&peripherals.SYSTMR);
                    legacy_print_string!(
                        &mut frame_sink,
                        "[device]: session expired after {packet_elapsed:?}, dumping."
                    );
                    flush_to_fifo(&mut frame_sink, uart);
                    mini_uart1_flush_tx(uart);
                    timeouts = Timeouts::new_8n1(INITIAL_BAUD_RATE);

                    protocol = ProtocolEnum::Handshake(Handshake::default());

                    ReceiveState::Waiting { initial: true }
                } else if byte_elapsed >= timeouts.byte_read
                    && !matches!(state, ReceiveState::Waiting { initial: true })
                {
                    last_byte_received = Instant::now(&peripherals.SYSTMR);
                    ReceiveState::Waiting { initial: false }
                } else {
                    state
                }
            }
        };

        if byte.is_some() {
            last_byte_received = Instant::now(&peripherals.SYSTMR);
        }
    }
}

pub struct StaticBuffers<const TX: usize, const RX: usize, const PX: usize, const IX: usize> {
    transmit: UnsafeCell<[u8; TX]>,
    receive: UnsafeCell<[u8; RX]>,
    staging: UnsafeCell<[u8; PX]>,
    cobs: UnsafeCell<[u8; 255]>,
    inflate: UnsafeCell<[u8; IX]>,
}
impl<const TX: usize, const RX: usize, const PX: usize, const IX: usize>
    StaticBuffers<TX, RX, PX, IX>
{
    pub const fn new() -> Self {
        Self {
            transmit: UnsafeCell::new([0u8; TX]),
            receive: UnsafeCell::new([0u8; RX]),
            staging: UnsafeCell::new([0u8; PX]),
            cobs: UnsafeCell::new([0u8; 255]),
            inflate: UnsafeCell::new([0u8; IX]),
        }
    }
    unsafe fn get(&self) -> AllocatedBuffers {
        // SAFETY:
        unsafe fn materialize<const N: usize>(b: &UnsafeCell<[u8; N]>) -> &'static mut [u8] {
            (*b.get()).as_mut_slice()
        }
        AllocatedBuffers {
            receive_buffer: materialize(&self.receive),
            transmit_buffer: materialize(&self.transmit),
            staging_buffer: materialize(&self.staging),
            cobs_encode_buffer: materialize(&self.cobs),
            inflate_buffer: materialize(&self.inflate),
        }
    }
    // unsafe fn get_inflate_buffer(&self) -> &mut [u8] {
    //     (*self.inflate.get()).as_mut_slice()
    // }
}
unsafe impl<const TX: usize, const RX: usize, const PX: usize, const IX: usize> Sync
    for StaticBuffers<TX, RX, PX, IX>
{
}
static STATIC_BUFFERS: StaticBuffers<0x10000, 0x10000, 0x10000, 0x20000> = StaticBuffers::new();
struct AllocatedBuffers<'a> {
    pub receive_buffer: &'a mut [u8],
    pub transmit_buffer: &'a mut [u8],
    pub staging_buffer: &'a mut [u8],
    pub cobs_encode_buffer: &'a mut [u8],
    pub inflate_buffer: &'a mut [u8],
}

// #[derive(Debug, Copy, Clone)]
// pub struct RawBufferConfig {
//     /// Memory (in bytes) to use for the receive buffer.
//     pub receive: usize,
//     /// Memory (in bytes) to use for the transmit buffer.
//     pub transmit: usize,
//     pub staging: usize,
//     pub inflate: usize,
// }
// struct BufferArena {
//     buffers: Option<AllocatedBuffers<'static>>,
//
//     exposed_base: *mut u8,
//     unsafe_end_of_buffers: *const (),
//     pub unsafe_memory_ends: *const (),
// }
// impl BufferArena {
//     unsafe fn new(base: usize, config: RawBufferConfig) -> Self {
//         let exposed_base: *mut u8 = core::ptr::with_exposed_provenance_mut(base);
//         let required_memory = config.receive
//             + config.transmit
//             + config.staging
//             + COBS_ENCODE_BUFFER_SIZE
//             + config.inflate;
//         let receive_buffer_ptr = exposed_base;
//         let transmit_buffer_ptr = exposed_base.add(config.receive);
//         let staging_buffer_ptr = transmit_buffer_ptr.add(config.transmit);
//         let cobs_buffer_ptr = staging_buffer_ptr.add(config.staging);
//         let inflate_buffer_ptr = cobs_buffer_ptr.add(config.inflate);
//         let receive_buffer = core::slice::from_raw_parts_mut(receive_buffer_ptr, config.receive);
//         let transmit_buffer = core::slice::from_raw_parts_mut(transmit_buffer_ptr, config.transmit);
//         let staging_buffer = core::slice::from_raw_parts_mut(staging_buffer_ptr, config.staging);
//         let cobs_encode_buffer =
//             core::slice::from_raw_parts_mut(cobs_buffer_ptr, COBS_ENCODE_BUFFER_SIZE);
//         let inflate_buffer = core::slice::from_raw_parts_mut(inflate_buffer_ptr, config.inflate);
//
//         let buffers = Some(AllocatedBuffers {
//             receive_buffer,
//             transmit_buffer,
//             staging_buffer,
//             cobs_encode_buffer,
//             inflate_buffer,
//         });
//         let unsafe_end_of_buffers = exposed_base.add(required_memory).cast();
//         let unsafe_memory_ends = core::ptr::without_provenance(512 * 1024 * 1024);
//
//         Self {
//             buffers,
//             exposed_base,
//             unsafe_end_of_buffers,
//             unsafe_memory_ends,
//         }
//     }
//
//     pub fn take(&mut self) -> Option<AllocatedBuffers> {
//         self.buffers.take()
//     }
// }

#[derive(Debug, Error, Copy, Clone)]
#[non_exhaustive]
pub enum ReceiveError {
    #[error("incoming message overflowed receive buffer")]
    BufferOverflow,
    #[error("incoming message overran the FIFO")]
    FifoOverrun,
    #[error("protocol error")]
    Protocol,
    #[error("error decoding message: {0}")]
    Decode(FrameError),
}

#[derive(Debug, Copy, Clone)]
struct ErrorState {
    at_instant: Instant,
    receive_error: Option<ReceiveError>,
}

#[derive(Debug, Copy, Clone)]
pub struct Timeouts {
    pub error_recovery: Duration,
    pub byte_read: Duration,
    pub session_expires: Duration,
    pub override_session_timeout: Option<Duration>,
}
impl Timeouts {
    pub fn new_8n1(baud: u32) -> Timeouts {
        Self {
            error_recovery: timeouts::ERROR_RECOVERY.at_baud_8n1(baud),
            byte_read: timeouts::BYTE_READ.at_baud_8n1(baud),
            session_expires: timeouts::SESSION_EXPIRES.at_baud_8n1(baud),
            override_session_timeout: None,
        }
    }
}

#[derive(Debug)]
pub enum ProtocolStatus {
    // Condition normal, continue with protocol.
    Continue,
    // Condition abnormal, abort processing of current packet and wait for retransmission.
    Abcon,
    // Abnormal end, abort all processing and return to initial state.
    Abend,
    Switch(ProtocolEnum),
}
