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

use alloc::vec;
use alloc::vec::Vec;
use bcm2835_lpa::{Peripherals, GPIO};
use bismuth::arch::arm1176::__dsb;
use bismuth::arch::arm1176::pmm::{RegionKind, PMM};
use bismuth::arch::arm1176::timing::Instant;
use bismuth::boot::PMM;
use bismuth::data::circular_buffer::CircularBuffer;
use bismuth::kalloc::SimpleAlloc;
use bismuth::sync::once::OnceLock;
use bismuth::{uart1_sendln_bl, MiB};
use core::alloc::{GlobalAlloc, Layout};
use core::time::Duration;

#[no_mangle]
pub extern "C" fn __bis__main() {
    uart1_sendln_bl!("=== RPI-DOWNLOADMOARRAM PASSTHROUGH ===");
    GLOBAL_ALLOC.0.get_or_init(|| {
        SimpleAlloc::new(
            (&mut PMM.get().lock())
                .allocate_region(RegionKind::Supersection)
                .unwrap(),
            16 * MiB,
        )
    });
    let peri = unsafe { Peripherals::steal() };

    __dsb();
    unsafe {
        peri.GPIO
            .gpfsel0()
            .modify(|_, w| w.fsel8().input().fsel9().input());
        peri.GPIO.gpfsel1().modify(|_, w| {
            w.fsel10()
                .input()
                .fsel11()
                .input()
                .fsel18()
                .input()
                .fsel19()
                .input()
        });
        peri.GPIO.gpfsel2().modify(|_, w| {
            w.fsel20()
                .input()
                .fsel21()
                .input()
                .fsel24()
                .input()
                .fsel25()
                .input()
        });
        // Clock falling
        peri.GPIO
            .gpfen0()
            .modify(|_, w| w.fen24().set_bit().fen25().set_bit());
        // peri.GPIO.gpren0()
        //     .modify(|_, w| w.ren24().set_bit().ren25().set_bit());
        // CS level low
        // peri.GPIO.gplen0()
        //     .modify(|_, w| w.len22().set_bit())

        // want to detect:
        //  CS level low
        //  clock falling
        //  read FAKE_MISO
    }
    __dsb();

    struct LineBufferedPassthrough {
        buf: CircularBuffer,
        newline_count: usize,
        byte: u8,
        status: bool,
    }
    impl LineBufferedPassthrough {
        pub fn new() -> Self {
            let buf = CircularBuffer::new(vec![0; 0x100000].leak());
            Self {
                buf,
                newline_count: 0,
                byte: 0,
                status: false,
            }
        }
        pub fn push(&mut self, n: u8) {
            if self.status {
                let b = (self.byte << 4) | n;
                self.buf.push_byte(b);
                if b == b'\n' {
                    self.newline_count += 1;
                }
                self.status = false;
            } else {
                self.byte = n;
                self.status = true;
            }
        }
    }
    fn poll(gpio: &GPIO, left: &mut LineBufferedPassthrough, right: &mut LineBufferedPassthrough) {
        __dsb();
        let eds = gpio.gpeds0().read();
        if eds.eds24().bit_is_set() || eds.eds25().bit_is_set() {
            unsafe {
                gpio.gpeds0()
                    .write_with_zero(|w| w.eds24().clear_bit_by_one().eds25().clear_bit_by_one())
            }
            let lev = gpio.gplev0().read().bits();
            if eds.eds24().bit_is_set() {
                let nybble = (lev >> 18) & 0x0f;
                left.push(nybble as u8);
            }
            if eds.eds25().bit_is_set() {
                let nybble = (lev >> 8) & 0x0f;
                right.push(nybble as u8);
            }
        }
        __dsb();
    }

    let mut left = LineBufferedPassthrough::new();
    let mut right = LineBufferedPassthrough::new();
    let mut which = 0;
    loop {
        poll(&peri.GPIO, &mut left, &mut right);
        __dsb();
        let lsr = peri.UART1.lsr().read();
        if lsr.tx_empty().bit_is_set() {
            if which == 3 {
                if let Some(b) = left.buf.shift_byte() {
                    unsafe { peri.UART1.io().write_with_zero(|w| w.data().variant(b)) }
                    if b == b'\n' {
                        left.newline_count -= 1;
                        which = 2;
                    }
                }
            } else if which == 4 {
                if let Some(b) = right.buf.shift_byte() {
                    unsafe { peri.UART1.io().write_with_zero(|w| w.data().variant(b)) }
                    if b == b'\n' {
                        right.newline_count -= 1;
                        which = 1;
                    }
                }
            } else {
                let (lav, rav) = (left.newline_count > 0, right.newline_count > 0);
                if lav && rav {
                    which = if which == 1 || which == 0 {
                        3 // left
                    } else {
                        4 // right
                    };
                } else {
                    if lav {
                        which = 3
                    } else if rav {
                        which = 4
                    }
                }

                if which == 3 {
                    unsafe { peri.UART1.io().write_with_zero(|w| w.data().variant(b'>')) }
                } else if which == 4 {
                    unsafe { peri.UART1.io().write_with_zero(|w| w.data().variant(b'<')) }
                }
            }
        }
        __dsb();
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
