#![feature(panic_info_message)]
#![feature(error_in_core)]
#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(try_blocks)]
#![feature(thread_local)]
#![feature(pointer_is_aligned_to)]
#![feature(strict_provenance)]
#![feature(exposed_provenance)]
#![feature(slice_ptr_get)]
#![feature(array_ptr_get)]
#![no_std]

use core::fmt::Write;
use bcm2835_lpa::Peripherals;

pub mod arch;
mod boot;
pub mod symbols;
pub mod kalloc;
mod muart;
mod sync;

// #[thread_local] makes codegen for:
//  bl __aeabi_read_tp which returns the thread pointer
//  the thread pointer is then offset by the key

#[no_mangle]
pub extern "C" fn __symbol_kstart__() {
    boot::boot();

    loop {}
}

#[no_mangle]
pub extern "C" fn __symbol_reboot__() -> ! {
    // TODO

    loop {}
}

#[panic_handler]
fn panic(info: &::core::panic::PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        uart1_sendln_bl!("[device]: Panic occurred at file '{}' line {}:", loc.file(), loc.line());
    } else {
        uart1_sendln_bl!("[device]: Panic occurred at [unknown location]");
    }
    if let Some(msg) = info.message() {
        if muart::UartBlockingFmt.write_fmt(*msg).is_err() {
            uart1_sendln_bl!("[device]: [failed to write message to UART]");
        }
    } else {
        uart1_sendln_bl!("[device]: [no message]");
    }
    uart1_sendln_bl!("[device]: rebooting.");

    __symbol_reboot__();
}
