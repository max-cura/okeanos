use core::alloc::Layout;
use core::any::Any;
use core::fmt::{Debug, Formatter};
use core::intrinsics::unlikely;
use core::mem::MaybeUninit;
use bcm2835_lpa::Peripherals;
use enum_dispatch::enum_dispatch;
use core::time::Duration;
use crate::arm1176::__dsb;
use crate::heap::bump::BumpHeap;
use crate::reactor::circular_buffer::CircularBuffer;

pub mod circular_buffer;
pub mod receive_buffer;
pub mod io_theseus;
pub mod protocol_theseus;
pub mod log_uart1_raw;
pub mod protocol_relay;

// Logger, Io, Protocol, Reactor
// Protocol is switched by the reactor
// Logger and Io are used by the Protocol and interact with the reactor
// Logger and Io have internal state
// There is reactor-level shared state

pub trait Logger {
    fn writeln_fmt(
        &mut self,
        reactor: &mut Reactor,
        args: core::fmt::Arguments
    );
    fn write_fmt(&mut self, reactor: &mut Reactor, args: core::fmt::Arguments);
}

pub trait IoEncode: Debug {
    fn encode_type(&self) -> u32;
    fn encode_to_slice<'a,'b>(&'b self, buf: &'a mut [u8]) -> postcard::Result<&'a mut [u8]>;
    fn proxied_debug(&self) -> &dyn Debug;
}

pub trait IoEncodeMarker: MessageClass + serde::Serialize {}
impl<T: IoEncodeMarker + Debug> IoEncode for T {
    fn encode_type(&self) -> u32 {
        Self::MSG_TYPE
    }

    fn encode_to_slice<'a, 'b>(&'b self, buf: &'a mut [u8]) -> postcard::Result<&'a mut [u8]> {
        postcard::to_slice(self, buf)
    }

    fn proxied_debug(&self) -> &dyn Debug {
        self
    }
}

#[derive(Debug)]
pub struct IoTimeoutsRelative {
    error_recovery: RateRelativeTimeout,
    byte_read_timeout: RateRelativeTimeout,
    session_timeout: Option<RateRelativeTimeout>,
    session_timeout_long: Option<RateRelativeTimeout>,
}

impl IoTimeoutsRelative {
    pub const fn with_ir(self) -> IoTimeouts {
        let Self { error_recovery, byte_read_timeout, session_timeout, session_timeout_long } = self;
        IoTimeouts {
            error_recovery: error_recovery.with_ir(),
            byte_read_timeout: byte_read_timeout.with_ir(),
            session_timeout: {
                match (session_timeout, session_timeout_long) {
                    (None, None) => None,
                    (Some(st), Some(stl)) => {
                        Some(SessionTimeout { normal: st.with_ir(), long: stl.with_ir(), use_long: false })
                    }
                    (Some(st), None) | (None, Some(st)) => {
                        Some(SessionTimeout { normal: st.with_ir(), long: st.with_ir(), use_long: false })
                    }
                }
            }
            // session_timeout_long: session_timeout_long.map(RateRelativeTimeout::with_ir),
            // use_long_timeout: false,
        }
    }
}

#[derive(Debug)]
pub struct SessionTimeout {
    normal: Duration,
    long: Duration,
    use_long: bool,
}

#[derive(Debug)]
pub struct IoTimeouts {
    error_recovery: Duration,
    byte_read_timeout: Duration,
    session_timeout: Option<SessionTimeout>,
    // session_timeout: Option<Duration>,
    // session_timeout_long: Option<Duration>,
    // use_long_timeout: bool,
}

pub trait IoImpl: Io {
    const DRIVES_UART : bool;
}

/// This trait handles sending and receiving messages using some encoding.
pub trait Io: Any {
    fn io_set_timeouts(
        &mut self,
        timeouts: Option<IoTimeoutsRelative>,
        use_long_timeout: Option<bool>,
    );

    fn io_reset(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut dyn Logger,
    );

    fn io_queue_message(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut dyn Logger,
        message: &dyn IoEncode,
    ) -> bool;

    fn io_tick(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut dyn Logger,
        indicators: &dyn Indicators,
    ) -> Result<Option<&[u8]>, ProtocolFlow>;

    fn io_flush_blocking(
        &mut self,
        reactor: &mut Reactor,
    );
}

// struct FakeIo;
// impl Io for FakeIo {
//     fn io_set_timeouts(&mut self, timeouts: Option<IoTimeoutsRelative>, use_long_timeout: Option<bool>) {}
//     fn io_reset(&mut self, reactor: &mut Reactor, logger: &mut dyn Logger) {}
//     fn io_queue_message(&mut self, reactor: &mut Reactor, logger: &mut dyn Logger, message: &dyn IoEncode) -> bool { false }
//     fn io_tick(&mut self, reactor: &mut Reactor, logger: &mut dyn Logger, indicators: &dyn Indicators) -> Result<Option<&[u8]>, ProtocolFlow> { Ok(None) }
// }
//
// impl<'a> Into<&'a mut dyn Any> for &'a mut dyn Io {
//     fn into(self) -> &'a mut dyn Any {
//         let mut fake = MaybeUninit::<FakeIo>::uninit();
//         let (_data_ptr, vtable_ptr) = unsafe {
//             core::mem::transmute::<&mut dyn Any, (usize, usize)>(&mut fake)
//         };
//         let (data_ptr, _vtable_ptr) = unsafe {
//             core::mem::transmute::<&mut dyn Io, (usize, usize)>(self)
//         };
//         let real = unsafe {
//             core::mem::transmute::<(usize, usize), &mut dyn Any>((data_ptr, vtable_ptr))
//         };
//         real
//     }
// }

#[derive(Debug)]
pub enum ProtocolFlow {
    /// Continue handling messages.
    Continue,
    /// Abnormal condition, ignore packet.
    Abcon,
    /// Abnormal end, reset protocol state.
    Abend,
    /// Switch to a different protocol.
    SwitchProtocol(ProtocolEnum),
}

use protocol_theseus::BootProtocol;
use protocol_relay::RelayProtocol;
use theseus_common::theseus::{MessageClass, MessageTypeType};
use crate::sendln_blocking;
use crate::timeouts::RateRelativeTimeout;

#[enum_dispatch]
#[derive(Debug, Clone)]
pub enum ProtocolEnum {
    BootProtocol,
    RelayProtocol,
}

/// This trait specifies one end of a specific protocol.
#[enum_dispatch(ProtocolEnum)]
pub trait Protocol: Clone + Debug {
    /// Called when the reactor receives a packet.
    fn protocol_handle(
        &mut self,
        reactor: &mut Reactor,
        logger: &mut dyn Logger,
        msg: &[u8],
    ) -> ProtocolFlow;

    /// Called at every tick of the reactor.
    fn protocol_heartbeat(
        &mut self,
        reactor: &mut Reactor,
        io: &mut dyn Io,
        logger: &mut dyn Logger,
    ) -> ProtocolFlow;
}

impl ProtocolEnum {
    pub fn initial() -> Self {
        todo!()
    }
}

pub trait Indicators {
    fn io_did_write(&self, rz: &Reactor, wrote: bool);
    fn io_is_receiving(&self, rz: &Reactor, receiving: bool);
    fn io_input_overrun(&self, rz: &Reactor, overrun: bool);
}

#[derive(Debug)]
pub struct Env {
    pub __unsafe_program_end__: *mut u8,
    pub __unsafe_memory_end__: *mut u8,
}

const UART_BUFFER_LAYOUT : Layout = unsafe { Layout::from_size_align_unchecked(
    0x10_000, // 65536
    4
) };

pub struct Reactor {
    pub peri: Peripherals,

    pub env: Env,
    pub heap: BumpHeap,

    // Special treatment for the UART, since right now we're always logging to it, but at the same
    // it may need to be used by the Io functionality as well.
    pub uart_buffer: CircularBuffer,

    pub set_session_timeout: Option<bool>,
}

impl Debug for Reactor {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Reactor")
            .field("peri", &())
            .field("env", &self.env)
            .field("heap", &self.heap)
            .field("uart_buffer", &self.uart_buffer)
            .field("set_session_timeout", &self.set_session_timeout)
            .finish()
    }
}

#[derive(Debug, Copy, Clone)]
enum IoCondition {
    Normal,
    Error,
}

impl Reactor {
    pub fn new(
        peripherals: Peripherals,
        env: Env,
    ) -> Option<Self> {
        let mut heap = BumpHeap::new(
            env.__unsafe_program_end__,
            env.__unsafe_memory_end__,
        );
        let uart_cb_ptr = heap.allocate(UART_BUFFER_LAYOUT)?;
        let uart_cb_mu = unsafe { uart_cb_ptr.as_uninit_slice_mut::<'static>() };
        for i in 0..uart_cb_mu.len() {
            uart_cb_mu[i].write(0);
        }
        let uart_cb = unsafe { MaybeUninit::slice_assume_init_mut(uart_cb_mu) };
        let uart_cb = CircularBuffer::new(uart_cb);

        Some(Self {
            peri: peripherals,
            env,
            heap,
            uart_buffer: uart_cb,
            set_session_timeout: None,
        })
    }

    pub fn run<LOG: Logger, IO: IoImpl, IND: Indicators>(
        &mut self,
        logger: &mut LOG,
        io: &mut IO,
        indicators: &IND,
        initial_protocol: ProtocolEnum
    ) {
        let mut protocol = initial_protocol.clone();

        loop {
            if !IO::DRIVES_UART {
                __dsb();

                let lsr = self.peri.UART1.lsr().read();
                // name is wrong: bit is set if there is space for *at least one byte* in the TX
                // FIFO.
                let can_write = lsr.tx_empty().bit_is_set();
                if can_write {
                    if let Some(b) = self.uart_buffer.shift_byte() {
                        // sendln_blocking!("wrote byte {b}, LBE={:?}",
                        //     self.uart_buffer.circle());
                        self.peri.UART1.io().write(|w| {
                            unsafe { w.data().bits(b) }
                        });
                    }
                }

                __dsb();
            }

            match io.io_tick(self, logger, indicators) {
                // message received
                Ok(Some(msg)) => {
                    match protocol.protocol_handle(self, logger, msg) {
                        ProtocolFlow::Continue => {
                            /* ignore */
                        }
                        ProtocolFlow::Abcon => {
                            io.io_reset(self, logger);
                        }
                        ProtocolFlow::Abend => {
                            protocol = initial_protocol.clone();
                            io.io_reset(self, logger);
                        }
                        ProtocolFlow::SwitchProtocol(p_new) => {
                            protocol = p_new;
                        }
                    }
                },
                // status normal
                Ok(None) => {},
                Err(ProtocolFlow::Abend) => {
                    protocol = initial_protocol.clone();
                },
                Err(e) => { /* ignore */ }
            }

            if unlikely(self.set_session_timeout.is_some()) {
                io.io_set_timeouts(None, core::mem::replace(&mut self.set_session_timeout, None));
            }

            protocol.protocol_heartbeat(self, io, logger);
        }
    }
}
