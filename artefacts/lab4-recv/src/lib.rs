#![feature(panic_info_message)]
#![feature(error_in_core)]
#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(decl_macro)]
#![feature(iter_repeat_n)]
#![feature(ascii_char)]
#![feature(ascii_char_variants)]
#![no_std]

use bcm2835_lpa::Peripherals;
use reactor::blinken;

pub mod stub;

pub mod legacy;

mod reactor;
mod timeouts;

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

    if let Some(loc) = info.location() {
        legacy_print_string_blocking!(&peri.UART1, "[device]: Panic occurred at file '{}' line {}:", loc.file(), loc.line());
    } else {
        legacy_print_string_blocking!(&peri.UART1, "[device]: Panic occurred at [unknown location]");
    }
    if let Some(msg) = info.message() {
        use core::fmt::Write as _;
        let bub = unsafe {
            core::mem::transmute::<
                *mut legacy::fmt::TinyBuf<0x4000>,
                &mut legacy::fmt::TinyBuf<0x4000>
            >(core::ptr::addr_of_mut!(legacy::fmt::BOOT_UMSG_BUF))
        };
        bub.clear();
        if core::fmt::write(bub, *msg).is_err() {
            legacy_print_string_blocking!(&peri.UART1, "[device]: [failed to write message to format buffer]");
        }
        if legacy::fmt::UartWrite::new(&peri.UART1).write_str(bub.as_str()).is_err() {
            legacy_print_string_blocking!(&peri.UART1, "[device]: [failed to write message to uart]");
        }
    } else {
        legacy_print_string_blocking!(&peri.UART1, "[device]: [no message]");
    }
    legacy_print_string_blocking!(&peri.UART1, "[device]: rebooting.");

    __theseus_reboot()
}
