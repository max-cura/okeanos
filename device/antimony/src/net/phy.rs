// General channel config

use crate::arch::exception::TrapFrame;
use crate::net::hdlc::{HDLC_ESC, HDLC_ESC_XOR, HDLC_FLAG};
use crate::net::{Buffer, hexdump};
use crate::{define_exception_trampoline, println};
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use core::cell::UnsafeCell;
use core::hint::{likely, unlikely};
use core::ops::Deref;
use critical_section::{CriticalSection, Mutex};
use d1_pac::Peripherals;
use d1_pac::uart::RegisterBlock;

pub const CHANNEL_MAX_FRAMES: usize = 128;

/// Transmit helper for [`CHANNEL2_TX`].
pub fn send(buf: Buffer) {
    let peri = unsafe { Peripherals::steal() };
    // println!("ppp: > {}", hexdump(&buf.bytes[..buf.len]));
    critical_section::with(|cs| unsafe {
        CHANNEL2_TX
            .borrow(cs)
            .get()
            .as_mut_unchecked()
            .push(buf, &peri.UART2);
    });
}

/// Transmit channel for _PPP in HDLC-like Framing_ link
struct TxBuffer {
    buffer: Buffer,
    index: usize,
    pending_escape: bool,
}
pub struct XmitChannel {
    buffers: VecDeque<Box<TxBuffer>>,
    writing: bool,
    needs_escape: [bool; 256],
}
impl XmitChannel {
    const fn new() -> Self {
        Self {
            buffers: VecDeque::new(),
            writing: false,
            needs_escape: [false; 256],
        }
    }
    pub fn needs_escape(&mut self, c: u8) {
        self.needs_escape[c as usize] = true;
    }
    fn write_out<U: Deref<Target = d1_pac::uart::RegisterBlock>>(&mut self, uart: &U) -> bool {
        let mut curr_head = self.buffers.front_mut().map(Box::as_mut);
        // let mut nbytes = 0;
        // let mut nesc = 0;
        'buffers: while let Some(buf) = curr_head {
            let mut i = buf.index;
            'inner: loop {
                if uart.usr().read().tfnf().is_full() {
                    buf.index = i;
                    break 'buffers;
                }
                let byte = buf.buffer.bytes[i];
                if buf.pending_escape {
                    let esc_byte = HDLC_ESC_XOR ^ byte;
                    uart.thr().write(|w| w.thr().variant(esc_byte));
                    buf.pending_escape = false;
                    // nbytes += 1;
                    i += 1;
                } else if self.needs_escape[byte as usize]
                    && !((i == 0 || i == buf.buffer.len - 1) && byte == 0x7e)
                {
                    buf.pending_escape = true;
                    uart.thr().write(|w| w.thr().variant(HDLC_ESC));
                    // nesc += 1;
                } else {
                    // nbytes += 1;
                    i += 1;
                    uart.thr().write(|w| w.thr().variant(byte));
                }
                if i == buf.buffer.len {
                    break 'inner;
                }
            }

            curr_head = None;
            let _ = self.buffers.pop_front();
        }
        // println!("Wrote {nbytes} bytes (escaped {nesc})");
        self.buffers.is_empty()
    }
    fn push<U: Deref<Target = d1_pac::uart::RegisterBlock>>(&mut self, buffer: Buffer, uart: &U) {
        self.buffers.push_back(Box::new(TxBuffer {
            buffer,
            index: 0,
            pending_escape: false,
        }));
        // If we're not already writing, then start writing. If we fill up the whole FIFO without
        // finishing all our frames, then enable the THRE interrupt
        if !self.writing && self.buffers.is_empty() {
            if !self.write_out(uart) {
                // println!("enable ETBEI");
                self.writing = true;
                uart.ier().modify(|_, w| w.etbei().enable());
            }
        }
    }
}
#[inline(always)]
fn dump_uart_buffer<U: Deref<Target = d1_pac::uart::RegisterBlock>>(
    uart: &U,
    channel: &mut XmitChannel,
) {
    if channel.write_out(uart) {
        channel.writing = false;
        // println!("Finished writing");
        uart.ier().modify(|_, w| w.etbei().disable());
    }
}

/// Receive-channel for _PPP in HDLC-like Framing_ link.
pub struct RecvChannel {
    write_head: usize,
    finished_head: usize,
    read_head: usize,
    free_buffers: usize,
    in_packet: bool,
    last_esc: bool,
    buffers: [Buffer; CHANNEL_MAX_FRAMES],
}
impl RecvChannel {
    const fn new() -> Self {
        Self {
            write_head: 0,
            finished_head: 0,
            read_head: 0,
            free_buffers: CHANNEL_MAX_FRAMES,
            in_packet: false,
            last_esc: false,
            buffers: [const { Buffer::new() }; CHANNEL_MAX_FRAMES],
        }
    }
    pub fn free_buffer(&mut self) {
        self.read_head = (self.read_head + 1) % CHANNEL_MAX_FRAMES;
        self.free_buffers += 1;
    }
    fn try_alloc_buffer(&mut self) -> Option<usize> {
        let free_buffers = self.free_buffers;
        if free_buffers == 0 {
            // println!("DENY wh={} fh={} rh={} fb={} ip={} le={}", self.write_head,
            //     self.finished_head, self.read_head, self.free_buffers, self.in_packet,
            //     self.last_esc);
            None
        } else {
            // println!("ALLOC wh={} fh={} rh={} fb={} ip={} le={}", self.write_head,
            //     self.finished_head, self.read_head, self.free_buffers, self.in_packet,
            //     self.last_esc );
            let buf = self.write_head;
            self.write_head = (buf + 1) % CHANNEL_MAX_FRAMES;
            self.free_buffers = free_buffers - 1;
            Some(buf)
        }
    }
    pub fn read_head_buf(&self) -> Option<&Buffer> {
        // finished_head points to the first buffer that is not finished
        if self.free_buffers == CHANNEL_MAX_FRAMES || self.read_head == self.finished_head {
            None
        } else {
            // println!("READ wh={} fh={} rh={} fb={} ip={} le={}", self.write_head,
            //     self.finished_head, self.read_head, self.free_buffers, self.in_packet,
            //     self.last_esc );
            Some(&self.buffers[self.read_head])
        }
    }
}
#[inline(always)]
fn drain_uart_buffer<U: Deref<Target = d1_pac::uart::RegisterBlock>>(
    uart: &U,
    channel: &mut RecvChannel,
) {
    // let peri = unsafe { Peripherals::steal() };

    let mut in_packet = channel.in_packet;
    let mut denied = false;
    let mut buf = (CHANNEL_MAX_FRAMES + channel.write_head - 1) % CHANNEL_MAX_FRAMES;
    let mut last_esc = channel.last_esc;
    let mut bi = if in_packet {
        // println!("retrieved length from wh={} buf={}: {}", channel.write_head, buf,
        //     channel.buffers[buf].len);
        channel.buffers[buf].len
    } else {
        0
    };
    while uart.usr().read().rfne().is_not_empty() {
        let byte = uart.rbr().read().rbr().bits();
        if denied {
            continue;
        }
        // print!("{byte:02x} ");
        if unlikely(byte == HDLC_FLAG) {
            let did_finish_frame;
            if in_packet && bi > 1 {
                // println!("WRITE_LEN:{bi} wh={} fh={} rh={} fb={} ip={} le={}", buf,
                //     channel.finished_head, channel.read_head, channel.free_buffers, in_packet,
                //     last_esc);
                channel.buffers[buf].len = bi;
                bi = 0;
                did_finish_frame = true;
            } else {
                did_finish_frame = false;
            }
            // bi == 1 implies that the only thing in the packet is a 7e; if we hit a 7e just now
            // then it's a zero-length frame, and we silently discard as per 1662
            if bi != 1 {
                if in_packet && did_finish_frame {
                    channel.finished_head = (channel.finished_head + 1) % CHANNEL_MAX_FRAMES;
                    // println!("FIN_HD:{bi} wh={} fh={} rh={} fb={} ip={} le={}", buf,
                    //     channel.finished_head, channel.read_head, channel.free_buffers,
                    //     in_packet, last_esc);
                }
                // println!("TRY ALLOC:bi={bi} in_packet={in_packet} dff={did_finish_frame}");
                let Some(a_buf) = channel.try_alloc_buffer() else {
                    denied = true;
                    continue;
                };
                buf = a_buf;
                in_packet = true;
                last_esc = false;
                bi = 1;
                channel.buffers[buf].bytes[0] = byte;
                channel.buffers[buf].len = 1;
            }
            // while peri.UART0.usr().read().tfnf().bit_is_clear() {}
            // peri.UART0.thr().write(|w| w.thr().variant(byte));
            continue;
        } else if likely(in_packet) {
            let byte = if last_esc {
                last_esc = false;
                byte ^ HDLC_ESC_XOR
            } else if byte == HDLC_ESC {
                last_esc = true;
                continue;
            } else {
                byte
            };
            channel.buffers[buf].bytes[bi] = byte;
            bi += 1;
            // while peri.UART0.usr().read().tfnf().bit_is_clear() {}
            // peri.UART0.thr().write(|w| w.thr().variant(byte));
        } else {
            // ignore
            continue;
        }
        // okay, got byte
    }
    channel.in_packet = in_packet;
    if in_packet {
        // println!("WRITE_LEN2:{bi} wh={} fh={} rh={} fb={} ip={} le={}", buf,
        //     channel.finished_head, channel.read_head, channel.free_buffers, in_packet, last_esc);
        channel.buffers[buf].len = bi;
    }
}

pub static CHANNEL2_RX: Mutex<UnsafeCell<RecvChannel>> =
    Mutex::new(UnsafeCell::new(RecvChannel::new()));
pub static CHANNEL2_TX: Mutex<UnsafeCell<XmitChannel>> =
    Mutex::new(UnsafeCell::new(XmitChannel::new()));

/// Machine-mode external interrupt; handles UART interrupts
extern "C" fn mei_uart(_trap_mode: *mut TrapFrame) {
    let peri = unsafe { Peripherals::steal() };
    let cs = unsafe { CriticalSection::new() };

    // UART range: 18 (UART0) - 23 (UART5)
    // DMAC range: 66 (DMAC_NS)

    match peri.PLIC.mclaim().read().mclaim().bits() {
        18 => unsafe {
            let done = STDOUT_TX
                .borrow(cs)
                .get()
                .as_mut_unchecked()
                .write_out(&peri.UART0);
            if done {
                peri.UART0.ier().modify(|_, w| w.etbei().disable());
            }

            peri.PLIC.mclaim().write(|w| w.mclaim().variant(18))
        },
        20 => {
            let usr = peri.UART2.usr().read();
            if usr.rfne().is_not_empty() {
                drain_uart_buffer(&peri.UART2, unsafe {
                    CHANNEL2_RX.borrow(cs).get().as_mut_unchecked()
                });
            }
            if usr.tfnf().is_not_full() {
                dump_uart_buffer(&peri.UART2, unsafe {
                    CHANNEL2_TX.borrow(cs).get().as_mut_unchecked()
                });
            }

            peri.PLIC.mclaim().write(|w| w.mclaim().variant(20))
        }
        claim => {
            // anything else, we silently ignore; we only clear the claim register
            peri.PLIC.mclaim().write(|w| w.mclaim().variant(claim))
        }
    }
}
define_exception_trampoline!(pub _trap_mei -> mei_uart);

// -- section: uart stdout

#[derive(Debug)]
struct UartXmit {
    strings: VecDeque<String>,
    index: usize,
}
impl UartXmit {
    const fn new() -> Self {
        Self {
            strings: VecDeque::new(),
            index: 0,
        }
    }
    /// Returns true if done.
    pub fn write_out<U: Deref<Target = RegisterBlock>>(&mut self, uart: &U) -> bool {
        while let Some(head) = self.strings.front() {
            while self.index < head.len() {
                if uart.usr().read().tfnf().is_not_full() {
                    uart.thr()
                        .write(|w| w.thr().variant(head.as_bytes()[self.index]));
                    self.index += 1;
                } else {
                    return false;
                }
            }
            self.strings.pop_front();
            self.index = 0;
        }
        true
    }
}
static STDOUT_TX: Mutex<UnsafeCell<UartXmit>> = Mutex::new(UnsafeCell::new(UartXmit::new()));
pub fn write_string<S: ToString>(s: S) {
    ::critical_section::with(|cs| {
        let peri = unsafe { Peripherals::steal() };
        let xmit = unsafe { STDOUT_TX.borrow(cs).get().as_mut_unchecked() };
        xmit.strings.push_back(s.to_string());
        if !xmit.write_out(&peri.UART0) {
            peri.UART0.ier().modify(|_, w| w.etbei().enable());
        }
    })
}
