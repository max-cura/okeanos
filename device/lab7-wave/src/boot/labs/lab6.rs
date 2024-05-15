use core::arch::asm;
use bcm2835_lpa::Peripherals;
use crate::boot::fmt::Uart1;
use crate::uprintln;
use core::fmt::Write;
use crate::arch::barrier::data_synchronization_barrier;
use crate::timing::{__floating_time, delay_micros};

#[inline(always)]
fn cycle_read() -> u32 {
    let mut c: u32;
    unsafe {
        asm!("mrc p15, 0, {t0}, c15, c12, 1", t0 = out(reg) c)
    }
    c
}

extern "C" {
    static _lab6_ivec: [u32; 8];
}

pub fn lab6(uart: &mut Uart1) {
    uprintln!(uart, "starting waveform generator");

    use data_synchronization_barrier as __dsb;

    let peri = unsafe { Peripherals::steal() };

    let gpio = &peri.GPIO;
    // let cm_pwm = &peri.CM_PWM;
    let st = &peri.SYSTMR;
    // let pwm = &peri.PWM0;

    __dsb();
    gpio.gpfsel0().modify(|_, w| w.fsel4().gpclk0());
    // gpio.gpfsel1().modify(|_, w| w.fsel12().pwm0_0());
    __dsb();

    let cm_gp0ctl : *mut u32 = 0x20101070 as _;
    let cm_gp0div : *mut u32 = 0x20101074 as _;

    unsafe {
        // PLLC (? designation unclear) 1GHz
        cm_gp0ctl.write_volatile(0x5a00_0005);
    }
    delay_micros(st, 110000);
    while {
        (unsafe { cm_gp0ctl.read_volatile() } & 0x80) != 0
    } {}

    let divi = 1000;
    let divf = 0;
    let v = divf | (divi << 12) | 0x5a00_0000;
    unsafe {
        cm_gp0div.write_volatile(v)
    }
    unsafe {
        cm_gp0ctl.write_volatile(0x5a00_0015);
    }
    loop {}
}

