#![feature(panic_info_message)]
#![feature(error_in_core)]
#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(decl_macro)]
#![feature(iter_repeat_n)]
#![feature(ascii_char)]
#![feature(ascii_char_variants)]
#![no_std]

use core::fmt::Write;
use bcm2835_lpa::Peripherals;
use lab4_common::arm1176::__dsb;
use lab4_common::{muart, sendln_blocking, Uart};
use reactor::blinken;

pub mod stub;

mod reactor;

#[no_mangle]
pub extern "C" fn __theseus_init() {
    stub::zero_stub_bss();

    loop {
        reactor::run();
    }
}

#[no_mangle]
pub extern "C" fn __theseus_reboot() -> ! {
    // TODO reboot
    loop {}
}

#[panic_handler]
fn panic(info: &::core::panic::PanicInfo) -> ! {
    let peri = unsafe { Peripherals::steal() };

    let blinken = blinken::Blinken::init(&peri.GPIO);
    blinken._6(&peri.GPIO, true);
    blinken._8(&peri.GPIO, true);
    blinken._27(&peri.GPIO, true);
    blinken._47(&peri.GPIO, true);

    // muart::uart1_init(&peri.GPIO, &peri.AUX, &peri.UART1, 270);
    // muart::uart1_init(&peri.GPIO, &peri.AUX, &peri.UART1, 270);

    if let Some(loc) = info.location() {
        sendln_blocking!("[device]: Panic occurred at file '{}' line {}:", loc.file(), loc.line());
    } else {
        sendln_blocking!("[device]: Panic occurred at [unknown location]");
    }
    if let Some(msg) = info.message() {
        if Uart.write_fmt(*msg).is_err() {
            sendln_blocking!("[device]: [failed to write message to UART]");
        }
    } else {
        sendln_blocking!("[device]: [no message]");
    }
    sendln_blocking!("[device]: rebooting.");

    __theseus_reboot()
}
