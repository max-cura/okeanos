use bcm2835_lpa::Peripherals;
use crate::boot::fmt::Uart1;
use core::fmt::Write;
use crate::{panic, uprintln};
use crate::boot::labs::lab6::lab6_scope;
use crate::panic::PanicBehaviour;

pub mod uart1;
pub mod redzone;
pub mod fmt;
pub mod led;
pub mod panic_boot;
pub mod labs;

#[no_mangle]
#[used]
pub static mut MY_STATIC: usize = 5;

pub fn boot_init() {
    // TODO: make this panic-proof
    unsafe {
        redzone::bss_init();
    }

    panic::set_panic_behaviour(PanicBehaviour::BootHalt);

    unsafe {
        redzone::redzone_init();
    }

    // No `take` yet b/c we don't have critical_section support yet.
    // We are also the sole thread of execution and IRQs are disabled at this point.


    // let peripherals = Peripherals::take();
    let peripherals = unsafe { Peripherals::steal() };
    // if peripherals.is_none() {
    //     data_synchronization_barrier();
    //     let peripherals = unsafe { Peripherals::steal() };
    //     peripherals.GPIO.gpfsel2().modify(|_, w| w.fsel27().output());
    //     unsafe { peripherals.GPIO.gpset0().write_with_zero(|w| w.set27().set_bit()) };
    //     data_synchronization_barrier();
    // }

    // let peripherals = peripherals.unwrap();

    // let peripherals = unsafe { Peripherals::steal() };

    uart1::uart1_init(
        &peripherals.GPIO,
        &peripherals.AUX,
        &peripherals.UART1,
        115200,
    );

    panic::set_panic_behaviour(PanicBehaviour::BootDumpSerial);

    let mut ser = Uart1::new(&peripherals.UART1);

    uprintln!(ser, "thalassa has entered boot process");
    uprintln!(ser, "UART1 configured and initialized");

    // panic!("Oopsie");

    uprintln!(ser, "Dumping .bss section:");
    unsafe {
        let start = core::ptr::addr_of!(__tlss_bss_start__);
        let end = core::ptr::addr_of!(__tlss_bss_end__);

        uprintln!(ser, "__tlss_bss_start__={:#?}", start);
        uprintln!(ser, "__tlss_bss_end__={:#?}", end);
        let bss_len = { end.offset_from(start) } as usize;
        let slice = core::slice::from_raw_parts(start, bss_len);
        for i in 0..bss_len {
            uprintln!(ser, "word at {:#08x?} is {:#08x}", &slice[i] as *const _, slice[i]);
        }
    };
    uprintln!(ser, "Done");

    // labs::lab1::lab1(&mut ser);
    // labs::lab4-send::lab4-send(&mut ser, &peripherals.SPI0, &peripherals.GPIO);
    // labs::lab_ws2813::lab4(&mut ser, &peripherals.SYSTMR, &peripherals.PWM0, &peripherals.CM_PWM, &peripherals.GPIO);
    // labs::lab1::lab1(&mut ser);
    // labs::lab6::lab6(&mut ser);
    lab6_scope(&mut ser);
}

extern "C" {
    static __tlss_bss_start__ : u32;
    static __tlss_bss_end__ : u32;
}

