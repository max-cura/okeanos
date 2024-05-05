use core::alloc::Layout;
use core::fmt::{Debug};
use thiserror::Error;
use theseus_common::cobs::{BufferedEncoder, FeedState, LineDecoder};
use crate::{ir, sendln_blocking};
use crate::ir::{IrReceiver, IrRecvError, IrTransmitter};
use crate::reactor::circular_buffer::{CircularBuffer, FrameSink};
use crate::reactor::receive_buffer::ReceiveBuffer;
use crate::reactor::{Indicators, Io, IoEncode, IoImpl, IoTimeouts, IoTimeoutsRelative, Logger, ProtocolFlow, Reactor};
use crate::timing::Instant;

#[derive(Debug, Copy, Clone)]
#[repr(u32)]
enum DriverState {
    AwaitingProtocolBegin = 1,
    AwaitingNextPacket = 2,

    /* IR Driver has no legacy mode, so no 3/4/5 */

    Preamble1 = 6,
    Preamble2 = 7,
    Preamble3 = 8,

    FrameSize0 = 9,
    FrameSize1(u32) = 10,
    FrameSize2(u32) = 11,
    FrameSize3(u32) = 12,

    CobsFrame {
        /// Number of bytes (COBS-encoded) comprising the packet
        total_encoded_length: usize,
        /// Number of bytes received so far
        received_bytes: usize,
    } = 13,

    Error {
        /// Used to determine when to time out of the error state; logging should be performed at
        /// time of error.
        at_instant: Instant,
    } = 14,
}

const COBS_BUFFER_LAYOUT : Layout = unsafe { Layout::from_size_align_unchecked(
    0xff,
    1,
) };
const POSTCARD_ENCODE_BUFFER_LAYOUT : Layout = unsafe { Layout::from_size_align_unchecked(
    0x1000,
    4,
) };

pub struct IrDriver {
    pub frame_sink: FrameSink,
    input: ReceiveBuffer,
    ir_tx: IrTransmitter,
    ir_rx: IrReceiver,

    state: DriverState,

    cobs_decoder: LineDecoder,

    timeouts: IoTimeouts,
    last_packet: Instant,
    last_byte: Instant,
}

#[derive(Debug, Error)]
pub enum DriverError {
    #[error("receive error: {0}")]
    Receive(#[from] IrRecvError),
    #[error("invalid frame size byte: {0:#04x} at shift {1}")]
    InvalidFrameSizeByte(u8, u32),
    #[error("COBS frame longer (>=1 byte) than declared ({total_encoded_length})")]
    FrameOverflow { total_encoded_length: usize },
    #[error("COBS frame shorter ({received_bytes}) than declared ({total_encoded_length})")]
    FrameUnderflow { total_encoded_length: usize, received_bytes: usize },
    #[error("COBS frame ({total_encoded_length}) is declared longer than input buffer ({buffer_len})")]
    InvalidFrameSize { total_encoded_length: usize, buffer_len: usize },
    #[error("COBS frame has no room for CRCs: only {0} bytes")]
    InsufficientLength(usize),
    #[error("COBS frame has mismatched CRCs: frame declared {declared:#010x}, computed {computed:#010x}")]
    CrcMismatch { declared: u32, computed: u32 },
    #[error("Protocol error forced session reset")]
    Protocol,
}

impl IrDriver {
    pub fn new(
        reactor: &mut Reactor,
        ibuf_layout: Layout,
        obuf_layout: Layout,
        timeouts: IoTimeoutsRelative,
    ) -> Option<Self> {
        let mut ibuf_ptr = reactor.heap.allocate(ibuf_layout)?;
        let mut obuf_ptr = reactor.heap.allocate(obuf_layout)?;
        let mut obuf_cobs_ptr = reactor.heap.allocate(COBS_BUFFER_LAYOUT)?;
        let mut obuf_postcard_ptr = reactor.heap.allocate(POSTCARD_ENCODE_BUFFER_LAYOUT)?;

        let ibuf = unsafe { ibuf_ptr.as_mut() };
        let obuf = unsafe { obuf_ptr.as_mut() };
        let obuf_cobs = unsafe { obuf_cobs_ptr.as_mut() };
        let obuf_postcard = unsafe{ obuf_postcard_ptr.as_mut() };

        let cb = CircularBuffer::new(obuf);
        let be = BufferedEncoder::with_buffer(obuf_cobs, 0x55)?;
        let frame_sink = FrameSink::new(
            cb,
            be,
            obuf_postcard
        );
        let input = ReceiveBuffer::new(ibuf);

        unsafe {
            // requires guarantee: interrupts disabled; thus the unsafe
            ir::init(&reactor.peri.GPIO, &reactor.peri.PWM0, &reactor.peri.CM_PWM, &reactor.peri.SYSTMR);
        }

        let timeouts = timeouts.with_ir();

        let ir_tx = IrTransmitter::new(&reactor.peri.SYSTMR);
        // TODO: not 100% sure if error_recovery is the right timeout to use here
        let ir_rx = IrReceiver::new(&reactor.peri.SYSTMR, timeouts.error_recovery);

        Some(Self {
            frame_sink, input, ir_tx, ir_rx,
            state: DriverState::AwaitingProtocolBegin,
            cobs_decoder: LineDecoder::new(),
            timeouts,
            last_packet: Instant::now(&reactor.peri.SYSTMR),
            last_byte: Instant::now(&reactor.peri.SYSTMR),
        })
    }

    fn set_timeouts(&mut self, new_timeouts: IoTimeouts) {
        self.timeouts = new_timeouts;
    }

    // called when IrReceiver::tick errors
    fn handle_error<L: Logger + ?Sized>(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut L,
        error: DriverError,
    ) -> Result<!, ProtocolFlow> {
        self.cobs_decoder.reset();
        self.state = DriverState::Error {
            at_instant: Instant::now(&reactor.peri.SYSTMR),
        };
        logger.writeln_fmt(reactor, format_args!("io_theseus: encountered error: {error}"));
        Err(ProtocolFlow::Continue)
    }

    fn state_tick<L: Logger + ?Sized, I: Indicators + ?Sized>(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut L,
        _indicators: &I,
        maybe_byte: Option<u8>,
    ) -> Result<Option<&[u8]>, ProtocolFlow> {
        fn handle_frame_size_byte<const SHIFT: u32>(b: u8, ex: u32) -> Result<u32, DriverError> {
            if (b & 0xc0) != 0xc0 {
                Err(DriverError::InvalidFrameSizeByte(b, SHIFT))
            } else {
                let bits = b & 0x3f;
                Ok(ex | ((bits as u32) << SHIFT))
            }
        }

        let mut packet_finished = false;

        let new_state = match (maybe_byte, self.state) {
            (Some(0x55), DriverState::AwaitingProtocolBegin)
                | (Some(0x55), DriverState::AwaitingNextPacket)
                => DriverState::Preamble1,
            (Some(0x55), DriverState::Preamble1)
                => DriverState::Preamble2,
            (Some(0x55), DriverState::Preamble2)
                | (Some(0x55), DriverState::Preamble3)
                => DriverState::Preamble3,
            (Some(0x5e), DriverState::Preamble3)
                => DriverState::FrameSize0,
            (Some(x), DriverState::FrameSize0)
                => DriverState::FrameSize1(match handle_frame_size_byte::<0>(x, 0) {
                Ok(x) => x,
                Err(e) => self.handle_error(reactor, logger, e)?
            }),
            (Some(x), DriverState::FrameSize1(y))
                => DriverState::FrameSize2(match handle_frame_size_byte::<6>(x, y) {
                Ok(x) => x,
                Err(e) => self.handle_error(reactor, logger, e)?
            }),
            (Some(x), DriverState::FrameSize2(y))
                => DriverState::FrameSize3(match handle_frame_size_byte::<12>(x, y) {
                Ok(x) => x,
                Err(e) => self.handle_error(reactor, logger, e)?
            }),
            (Some(x), DriverState::FrameSize3(y))
                => {
                let total_encoded_length = match handle_frame_size_byte::<18>(x, y) {
                    Ok(x) => x,
                    Err(e) => self.handle_error(reactor, logger, e)?
                } as usize;
                if total_encoded_length >= self.input.len() {
                    // nope
                    self.handle_error(
                        reactor, logger, DriverError::InvalidFrameSize {
                            total_encoded_length, buffer_len: self.input.len() })?
                } else {
                    self.input.clear();
                    DriverState::CobsFrame {
                        total_encoded_length,
                        received_bytes: 0,
                    }
                }
            },
            (Some(x), DriverState::CobsFrame {total_encoded_length, received_bytes})
                => {
                if received_bytes >= total_encoded_length {
                    self.handle_error(
                        reactor,
                        logger,
                        DriverError::FrameOverflow { total_encoded_length },
                    )?
                } else {
                    let byte = x ^ 0x55;
                    match self.cobs_decoder.feed(byte) {
                        FeedState::PacketFinished => {
                            if total_encoded_length != (received_bytes + 1) {
                                // frame shorter than declared
                                self.handle_error(reactor, logger, DriverError::FrameUnderflow { total_encoded_length, received_bytes: received_bytes + 1 })?
                            }
                            packet_finished = true;
                            DriverState::AwaitingNextPacket
                        }
                        FeedState::Byte(b) => {
                            self.input.push_byte(b);
                            DriverState::CobsFrame {
                                total_encoded_length,
                                received_bytes: received_bytes + 1
                            }
                        }
                        FeedState::Pass => {
                            // continue
                            DriverState::CobsFrame {
                                total_encoded_length,
                                received_bytes: received_bytes + 1
                            }
                        }
                    }
                }
            },

            (_, errorstate @ DriverState::Error { at_instant }) => {
                if at_instant.elapsed(&reactor.peri.SYSTMR) < self.timeouts.error_recovery {
                    errorstate
                } else {
                    DriverState::AwaitingNextPacket
                }
            }
            (_, state) => {
                if matches!(state, DriverState::AwaitingProtocolBegin) {
                    DriverState::AwaitingProtocolBegin
                } else {
                    let packet_elapsed = self.last_packet.elapsed(&reactor.peri.SYSTMR);
                    let byte_elapsed = self.last_byte.elapsed(&reactor.peri.SYSTMR);

                    let hit_session_timeout = self.timeouts.session_timeout.as_ref().map(|st| {
                        packet_elapsed >= if st.use_long {
                            st.long
                        } else {
                            st.long
                        }
                    }).unwrap_or(false);

                    // let effective_session_timeout = if self.timeouts.use_long_timeout {
                    //     self.timeouts.session_timeout_long
                    // } else {
                    //     self.timeouts.session_timeout
                    // };
                    //
                    // let hit_session_timeout = packet_elapsed >= effective_session_timeout;
                    let hit_byte_timeout = byte_elapsed >= self.timeouts.byte_read_timeout;

                    if hit_session_timeout && hit_byte_timeout {
                        self.last_packet = Instant::now(&reactor.peri.SYSTMR);
                        let _ = logger.writeln_fmt(reactor, format_args!("[device]: session expired after {packet_elapsed:?}"));
                        if let Some(st) = self.timeouts.session_timeout.as_mut() {
                            st.use_long = false;
                        }
                        // will reset the protocol to default
                        self.state = DriverState::AwaitingProtocolBegin;
                        return Err(ProtocolFlow::Abend)
                    } else if hit_byte_timeout {
                        // can we get rid of this?
                        self.last_byte = Instant::now(&reactor.peri.SYSTMR);
                        DriverState::AwaitingNextPacket
                    } else {
                        state
                    }
                }
            }
        };

        if maybe_byte.is_some() {
            self.last_byte = Instant::now(&reactor.peri.SYSTMR);
        }

        self.state = new_state;

        if !packet_finished {
            return Ok(None)
        }

        let split_point : usize = match try {
            let packet = self.input.as_bytes();
            let split_point = match packet.len().checked_sub(4) {
                Some(i) => i,
                None => {
                    Err(DriverError::InsufficientLength(packet.len()))?
                },
            };
            let (data_frame_bytes, crc_bytes) = unsafe {
                packet.split_at_unchecked(split_point)
            };
            let crc_bytes : [u8; 4] = unsafe { crc_bytes.try_into().unwrap_unchecked() };
            let declared_crc = u32::from_le_bytes(crc_bytes);
            let computed_crc = crc32fast::hash(data_frame_bytes);
            if declared_crc != computed_crc {
                Err(DriverError::CrcMismatch {
                    declared: declared_crc,
                    computed: computed_crc,
                })?
            };
            split_point
        } {
            Ok(x) => x,
            Err(de) => self.handle_error(reactor, logger, de)?
        };

        self.last_packet = Instant::now(&reactor.peri.SYSTMR);

        let data_frame_bytes = &self.input.as_bytes()[..split_point];

        Ok(Some(data_frame_bytes))
    }

    pub fn use_long_session_timeout(&mut self, x: bool) {
        if let Some(st) = self.timeouts.session_timeout.as_mut() {
            st.use_long = x;
        }
    }

    pub fn flush(&mut self, reactor: &Reactor) {
        while let Some(b) = self.frame_sink._buffer_mut().shift_byte() {
            while !self.ir_tx.can_push() {
                self.ir_tx.tick(&reactor.peri.PWM0, &reactor.peri.SYSTMR);
            }
            self.ir_tx.try_push(b);

            // while self.ir_tx.byte.is_some() {
            //     self.ir_tx.tick(
            //         &reactor.peri.GPIO,
            //         &reactor.peri.SYSTMR,
            //     );
            // }
            // self.ir_tx.byte = Some(b);
        }

        while self.ir_tx.tick(&reactor.peri.PWM0, &reactor.peri.SYSTMR) {}
        // while !self.ir_tx.idle() {
        //     self.ir_tx.tick(
        //         &reactor.peri.GPIO,
        //         &reactor.peri.SYSTMR,
        //     )
        // }
    }
}

impl IoImpl for IrDriver {
    const DRIVES_UART: bool = false;
}

impl Io for IrDriver {
    fn io_set_timeouts(&mut self, timeouts: Option<IoTimeoutsRelative>, use_long_timeout: Option<bool>) {
        if let Some(t) = timeouts {
            self.timeouts = t.with_ir();
        }
        if let Some(b) = use_long_timeout {
            if let Some(st) = self.timeouts.session_timeout.as_mut() {
                st.use_long = b;
            }
        }
    }

    // called for Abend and Abcon - basically, enter error state
    fn io_reset(&mut self, reactor: &mut Reactor, logger: &mut dyn Logger) {
        let _ = self.handle_error(reactor, logger, DriverError::Protocol);
    }

    fn io_queue_message(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut dyn Logger,
        message: &dyn IoEncode
    ) -> bool {
        let _ = logger.writeln_fmt(reactor, format_args!("queued message: {:?}", message.proxied_debug()));
        match self.frame_sink.send_dyn(message) {
            Ok(x) => {
                let (b1,b2) = self.frame_sink._buffer().view();
                for b in b1.into_iter().copied().chain(b2.into_iter().copied()) {
                    let mut s = [0u8; 2];
                    static HELPER : &[u8] = b"0123456789abcdef";
                    s[0] = HELPER[{(b & 0xf0) >> 4} as usize];
                    s[1] = HELPER[(b & 0x0f) as usize];
                    logger.write_fmt(reactor, format_args!("{} ", core::str::from_utf8(&s).unwrap()));
                }
                logger.writeln_fmt(reactor, format_args!(""));
                x
            },
            Err(e) => {
                let _ = logger.writeln_fmt(
                    reactor,
                    format_args!("error queuing message for IR driver: {e}")
                );
                false
            }
        }
    }

    fn io_tick(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut dyn Logger,
        indicators: &dyn Indicators
    ) -> Result<Option<&[u8]>, ProtocolFlow> {
        let mut did_write = false;
        if self.ir_tx.can_push() {
            if let Some(b) = self.frame_sink._buffer_mut().shift_byte() {
                self.ir_tx.try_push(b);
                did_write = true;
            }
        }
        // if self.ir_tx.byte.is_none() {
        //     if let Some(b) = self.frame_sink._buffer_mut().shift_byte() {
        //         self.ir_tx.byte = Some(b);
        //         did_write = true;
        //     }
        // }
        indicators.io_did_write(reactor, did_write);

        self.ir_tx.tick(
            &reactor.peri.PWM0,
            &reactor.peri.SYSTMR,
        );

        match self.ir_rx.tick(
            &reactor.peri.GPIO,
            &reactor.peri.SYSTMR,
        ) {
            Ok(b) => {
                // if let Some(b) = b {
                //     logger.writeln_fmt(reactor, format_args!("{b:02x}"));
                // }
                indicators.io_is_receiving(reactor, b.is_some());
                self.state_tick(reactor, logger, indicators, b)
            }
            Err(ire) => {
                // todo: error indicator?
                indicators.io_is_receiving(reactor, false);
                self.handle_error(
                    reactor,
                    logger,
                    DriverError::Receive(ire),
                )?
            }
        }
    }

    fn io_flush_blocking(&mut self, reactor: &mut Reactor) {
        self.flush(reactor);
    }
}