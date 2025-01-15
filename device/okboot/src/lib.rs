#![allow(internal_features)]
#![feature(core_intrinsics)]
#![no_std]

use crate::protocol::RawBufferConfig;
use bcm2835_lpa::Peripherals;
use core::panic::PanicInfo;

pub mod arch;
mod buf;
mod io;
mod protocol;
mod stub;
pub mod timeouts;

#[no_mangle]
pub extern "C" fn __symbol_kstart() -> ! {
    // NOTE: It seems to be impractical/impossible to zero out the BSS in life-after-main, so we
    //       now do it in life-before-main (specifically, in _start in boot.S).
    // This is mostly because it is UB for the BSS to be uninitialized during AM execution, and also
    // because there is no way to get a pointer with provenance for the whole BSS section.

    let mut peripherals = unsafe { Peripherals::steal() };
    let buffer_config = RawBufferConfig {
        transmit: 0x10000,
        receive: 0x10000,
        staging: 0x10000,
    };
    loop {
        protocol::run(&mut peripherals, buffer_config);
    }
}

#[no_mangle]
pub extern "C" fn __symbol_kreboot() -> ! {
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    __symbol_kreboot();
}
