#![feature(panic_info_message)]
#![feature(error_in_core)]
#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(try_blocks)]
#![feature(thread_local)]
#![feature(pointer_is_aligned_to)]
#![feature(slice_ptr_get)]
#![feature(array_ptr_get)]
#![feature(format_args_nl)]
#![feature(allocator_api)]
#![feature(iter_repeat_n)]
#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::time::Duration;
use bcm2835_lpa::Peripherals;
use bismuth::arch::arm1176::__dsb;
use bismuth::arch::arm1176::pmm::{PMM, RegionKind};
use bismuth::arch::arm1176::timing::Instant;
use bismuth::boot::PMM;
use bismuth::kalloc::SimpleAlloc;
use bismuth::sync::once::OnceLock;
use bismuth::{MiB, uart1_sendln_bl};

#[derive(Clone)]
struct SoftwareUart {
    byte: u8,
    // 0 -> initial/waiting for byte
    // k -> (k-1)'th bit, little endian
    state: u8,
    msg_start: Instant,
    last_bit: Instant,
}

impl SoftwareUart {
    // 4/39 for 230400. However the drift arises from it being in point of fact 4.3us
    pub fn poll(
        &mut self,
        now: Instant,
        changed: bool,
        new_lev: bool,
        us_per_bit: u64,
        us_per_symbol: u64,
    ) -> Option<u8> {
        if changed {
            uart1_sendln_bl!("change: new_lev={new_lev}");
            let elapsed = self.last_bit.elapsed_to(now);
            let from = !new_lev;
            self.last_bit = now;
            if self.state == 0 && from {
                self.state = 1;
                self.byte = 0;
                self.msg_start = now;
            } else {
                let elapsed_bits = ((elapsed.as_micros() as u64) / us_per_bit) as u8;
                if from {
                    let x = 0xff >> (8 - elapsed_bits);
                    self.byte |= x << (self.state - 1);
                }
                self.state += elapsed_bits;
            }
        }
        if self.state > 0 && self.msg_start.elapsed_to(now) > Duration::from_micros(us_per_symbol) {
            let b = self.byte;
            self.byte = 0;
            self.state = 0;
            uart1_sendln_bl!("byte: {b}");
            Some(b)
        } else {
            None
        }
    }
}

#[no_mangle]
pub extern "C" fn __bis__main() {
    uart1_sendln_bl!("=== RPI-DOWNLOADMOARRAM PASSTHROUGH ===");
    GLOBAL_ALLOC.0.get_or_init(|| {
        SimpleAlloc::new((&mut PMM.get().lock()).allocate_region(RegionKind::Supersection).unwrap(), 16 * MiB)
    });
    let peri = unsafe { Peripherals::steal() };

    __dsb();
    unsafe {
        peri.GPIO.gpfsel2()
            .modify(|_, w|
                w.fsel24().input().fsel25().input());
        peri.GPIO.gpfen0()
            .modify(|_, w| w.fen24().set_bit().fen25().set_bit());
        peri.GPIO.gpren0()
            .modify(|_, w| w.ren24().set_bit().ren25().set_bit());
    }
    __dsb();
    // input clocks: 57600 each
    // so 8.6us per bit, and 77.4
    let clk1 = 9;
    let clk2 = 77;

    let mut uart24 = SoftwareUart {
        byte: 0,
        state: 0,
        msg_start: Instant::now(&peri.SYSTMR),
        last_bit: Instant::now(&peri.SYSTMR),
    };
    let mut uart25 = SoftwareUart {
        byte: 0,
        state: 0,
        msg_start: Instant::now(&peri.SYSTMR),
        last_bit: Instant::now(&peri.SYSTMR),
    };
    let mut cb24 = bismuth::data::circular_buffer::CircularBuffer::new(Vec::from_iter(core::iter::repeat_n(0u8, 0x100000)).leak());
    let mut cb25 = bismuth::data::circular_buffer::CircularBuffer::new(Vec::from_iter(core::iter::repeat_n(0u8, 0x100000)).leak());
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    enum Select {
        None,
        Buf24,
        Buf25,
    }
    let mut select = Select::None;
    let mut nlc24 = 0;
    let mut nlc25 = 0;
    loop {
        if select != Select::None {
            __dsb();
            let lsr = peri.UART1.lsr().read();
            let can_write = lsr.tx_empty().bit_is_set();
            __dsb();
            if can_write {
                match select {
                    Select::None => {unreachable!()}
                    Select::Buf24 => {
                        if let Some(b) = cb24.shift_byte() {
                            __dsb();
                            peri.UART1.io().write(|w| unsafe { w.data().bits(b) });
                            __dsb();
                            if b == b'\n' {
                                nlc24 -= 1;
                            }
                            if nlc24 == 0 {
                                select = if nlc25 > 0 {
                                    Select::Buf25
                                } else {
                                    Select::None
                                }
                            }
                        }
                    }
                    Select::Buf25 => {
                        if let Some(b) = cb25.shift_byte() {
                            __dsb();
                            peri.UART1.io().write(|w| unsafe { w.data().bits(b) });
                            __dsb();
                            if b == b'\n' {
                                nlc25 -= 1;
                                if nlc25 == 0 {
                                    select = if nlc24 > 0 {
                                        Select::Buf24
                                    } else {
                                        Select::None
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let now = Instant::now(&peri.SYSTMR);

        let eds0 = peri.GPIO.gpeds0().read();
        let change24 = eds0.eds24().bit_is_set();
        let change25 = eds0.eds25().bit_is_set();

        let lev = peri.GPIO.gplev0().read();

        if change24 || change25 {
            unsafe { peri.GPIO.gpeds0().write_with_zero(|w| w.eds24().clear_bit_by_one().eds25().clear_bit_by_one()) };
        }

        if let Some(b24) = uart24.poll(now, change24, lev.lev24().bit(), clk1, clk2) {
            cb24.push_byte(b24);
            if b24 == b'\n' {
                nlc24 += 1;
            }
        }
        if let Some(b25) = uart25.poll(now, change25, lev.lev25().bit(), clk1, clk2) {
            cb25.push_byte(b25);
            if b25 == b'\n' {
                nlc25 += 1;
            }
        }
    }
}

/* */
pub struct SimpleGlobal(pub(crate) OnceLock<SimpleAlloc>);

unsafe impl GlobalAlloc for SimpleGlobal {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.0.get().unwrap().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.0.get().unwrap().dealloc(ptr, layout)
    }
}

#[no_mangle]
pub extern "C" fn __aeabi_unwind_cpp_pr0() {}

#[global_allocator]
pub(crate) static GLOBAL_ALLOC: SimpleGlobal = SimpleGlobal(OnceLock::new());
/* */
