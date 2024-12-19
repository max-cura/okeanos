#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(iter_repeat_n)]
#![no_std]

use core::panic::PanicInfo;

pub mod arch;
mod buf;
pub mod timeouts;

#[no_mangle]
pub extern "C" fn __symbol_kstart() -> ! {
    //

    __symbol_kreboot();
}

#[no_mangle]
pub extern "C" fn __symbol_kreboot() -> ! {
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    //
    __symbol_kreboot();
}
