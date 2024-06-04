#![feature(iter_repeat_n)]
#![feature(try_blocks)]
#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(allocator_api)]
#![feature(ptr_as_uninit)]
#![feature(maybe_uninit_slice)]
#![feature(error_in_core)]
#![feature(never_type)]
#![no_std]

use crate::arm1176::__dsb;
use bcm2835_lpa::Peripherals;
use core::fmt::Write;

pub mod arm1176;
pub mod heap;
pub mod ir;
pub mod muart;
pub mod reactor;
pub mod relocation;
pub mod symbols;
pub mod timeouts;
pub mod timing;

#[macro_export]
macro_rules! sendln_blocking {
    ($($args:tt)*) => {
        {
            use core::fmt::Write as _;
            let mut uart = $crate::Uart;
            let _ = writeln!(uart, $($args)*);
        }
    }
}
#[macro_export]
macro_rules! send_blocking {
    ($($args:tt)*) => {
        {
            use core::fmt::Write as _;
            let mut uart = $crate::Uart;
            let _ = write!(uart, $($args)*);
        }
    }
}

pub struct Uart;

impl Write for Uart {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let peri = unsafe { Peripherals::steal() };
        let uart = &peri.UART1;

        __dsb();

        for &b in s.as_bytes() {
            while uart.stat().read().tx_ready().bit_is_clear() {}
            unsafe { uart.io().write_with_zero(|w| unsafe { w.data().bits(b) }) };
        }

        __dsb();

        Ok(())
    }
}
