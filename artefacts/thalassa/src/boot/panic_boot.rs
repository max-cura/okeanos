use core::panic::PanicInfo;
use bcm2835_lpa::Peripherals;
use crate::boot::fmt::Uart1;
use crate::panic::PanicFn;
use crate::uprintln;
use core::fmt::Write;

pub static BOOT_PANIC_HALT : PanicFn = boot_panic_halt;
pub static BOOT_PANIC_SERIAL : PanicFn = boot_panic_serial;

pub fn boot_panic_serial(info: &PanicInfo) -> ! {
    let peripherals = unsafe { Peripherals::steal() };
    let mut ser = Uart1::new(&peripherals.UART1);
    if let Some(loc) = info.location() {
        uprintln!(ser, "Panic occurred at file '{}' line {}:", loc.file(), loc.line());
    } else {
        uprintln!(ser, "Panic occurred at [unknown location]");
    }
    if let Some(msg) = info.message() {
        if core::fmt::write(&mut ser, *msg).is_err() {
            uprintln!(ser, "[failed to write message]");
        }
    } else {
        uprintln!(ser, "[no message]");
    }

    boot_panic_halt(info)
}

pub fn boot_panic_halt(_: &PanicInfo) -> ! {
    loop {}
}