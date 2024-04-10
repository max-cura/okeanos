use bcm2835_lpa::Peripherals;
use crate::boot::fmt::Uart1;
use core::fmt::Write;
use crate::{panic, uprintln};
use crate::boot::panic_boot::{BOOT_PANIC_HALT, BOOT_PANIC_SERIAL};

mod uart1;
mod redzone;
mod fmt;
mod led;
mod panic_boot;

pub fn boot_init() {
    panic::set_panic_behaviour(&BOOT_PANIC_HALT as *const _);

    unsafe {
        redzone::redzone_init();
    }

    // no `take` yet b/c we don't have critical_section support yet
    let peripherals = unsafe { Peripherals::steal() };

    uart1::uart1_init(
        &peripherals.GPIO,
        &peripherals.AUX,
        &peripherals.UART1,
        115200,
    );

    panic::set_panic_behaviour(&BOOT_PANIC_SERIAL as *const _);

    let mut ser = Uart1::new(&peripherals.UART1);

    uprintln!(ser, "thalassa has entered boot process");
    uprintln!(ser, "UART1 configured and initialized");

    panic!("Oopsie");
}
