use crate::exceptions::VectorBuilder;
use crate::int::{self, InterruptMode, OperatingMode};
use crate::{def_irq_landing_pad, steal_println};
use bcm2835_lpa::Peripherals;
use core::alloc::Layout;
use core::arch::global_asm;
use quartz::arch::arm1176::__dsb;
use quartz::device::bcm2835::timing::delay_millis;

def_irq_landing_pad!(local_irq, handle_irq);

fn handle_irq() {
    let peripherals = unsafe { Peripherals::steal() };
    let pending2 = peripherals.LIC.pending_2().read();
    if pending2.gpio_0().bit_is_set() {
        let mask = peripherals.GPIO.gpeds0().read().bits();
        let bits = peripherals.GPIO.gplev0().read().bits();
        unsafe { peripherals.GPIO.gpeds0().write_with_zero(|w| w.bits(mask)) };
        steal_println!("bits={bits:b} mask={mask:b}");
    }
}

pub fn run(peripherals: &Peripherals) {
    steal_println!("allocating vector destination");

    // allocate space for vector table
    let layout = Layout::array::<u32>(8).unwrap();
    let vdi_ptr: *mut u32 = core::ptr::with_exposed_provenance_mut(0);
    let vdi_len = layout.size();
    let vector_dst = core::ptr::slice_from_raw_parts_mut(vdi_ptr, vdi_len);

    steal_println!("clearing LIC IRQs");

    __dsb();
    peripherals
        .LIC
        .disable_1()
        .write(|w| unsafe { w.bits(0xffff_ffff) });
    peripherals
        .LIC
        .disable_2()
        .write(|w| unsafe { w.bits(0xffff_ffff) });
    peripherals
        .LIC
        .disable_basic()
        .write(|w| unsafe { w.bits(0xffff_ffff) });
    __dsb();

    steal_println!("configuring GPIO");

    // GPIO[31:16,13,11:0] are INPUT
    // GPIO[14,15] are TXD0, RXD0
    // GPIO[13] is PWM1 (channel 2)
    __dsb();
    unsafe {
        steal_println!("configuring GPIO - FSEL0");
        peripherals
            .GPIO
            .gpfsel0()
            .write_with_zero(|w| w.bits(0o00000_00000));
        steal_println!("configuring GPIO - FSEL1");
        peripherals
            .GPIO
            .gpfsel1()
            .write_with_zero(|w| w.bits(0o00002_24400));
        steal_println!("configuring GPIO - FSEL2");
        peripherals
            .GPIO
            .gpfsel2()
            .write_with_zero(|w| w.bits(0o00000_00000));
        steal_println!("configuring GPIO - FSEL3");
        peripherals
            .GPIO
            .gpfsel3()
            .modify(|_, w| w.fsel30().input().fsel31().input());
        steal_println!("configuring GPIO - xxEN");
        peripherals
            .GPIO
            .gpfen0()
            .write_with_zero(|w| unsafe { w.bits(0xffff_0fff) });
        peripherals
            .GPIO
            .gpren0()
            .write_with_zero(|w| unsafe { w.bits(0xffff_0fff) });
        peripherals.GPIO.gpafen0().write_with_zero(|w| w);
        peripherals.GPIO.gparen0().write_with_zero(|w| w);
        peripherals.GPIO.gphen0().write_with_zero(|w| w);
        peripherals.GPIO.gplen0().write_with_zero(|w| w);
        steal_println!("configuring GPIO - EDS");
        peripherals
            .GPIO
            .gpeds0()
            .write_with_zero(|w| w.bits(0xffff_ffff))
    }
    steal_println!("enabling LIC bank 2");
    __dsb();
    peripherals.LIC.enable_2().write(|w| w.gpio_0().set_bit());
    __dsb();

    steal_println!("installing vector table");
    // fill in vector table
    let vector = VectorBuilder::new().set_irq_handler(unsafe { &raw const local_irq });
    unsafe { vector.install(vector_dst) }.unwrap();

    steal_println!("initializing IRQ stack");
    unsafe { int::init_stack_for_mode(OperatingMode::IRQ, 0x0800_0000) };

    // pwm init
    steal_println!("initializing PWM0 on channel 2");
    __dsb();
    peripherals
        .CM_PWM
        .cs()
        .write(|w| w.passwd().passwd().src().xosc());
    __dsb();
    delay_millis(&peripherals.SYSTMR, 110);
    __dsb();
    while peripherals.CM_PWM.cs().read().busy().bit_is_set() {
        delay_millis(&peripherals.SYSTMR, 1);
    }
    peripherals
        .CM_PWM
        .div()
        .write(|w| w.passwd().passwd().divi().variant(16));
    peripherals
        .CM_PWM
        .cs()
        .write(|w| w.passwd().passwd().enab().set_bit().src().xosc());
    __dsb();
    peripherals.PWM0.ctl().modify(|_, w| {
        w.mode1()
            .pwm()
            .msen1()
            .set_bit()
            .mode2()
            .pwm()
            .msen2()
            .set_bit()
            .pwen1()
            .set_bit()
            .pwen2()
            .set_bit()
    });
    peripherals.PWM0.rng1().write(|w| unsafe { w.bits(1024) });
    peripherals.PWM0.dat1().write(|w| unsafe { w.bits(32) });
    peripherals.PWM0.rng2().write(|w| unsafe { w.bits(1024) });
    peripherals.PWM0.dat2().write(|w| unsafe { w.bits(256) });
    __dsb();

    steal_println!("done");

    // test input
    let save = int::set_enabled_interrupts(InterruptMode::IrqOnly);
    delay_millis(&peripherals.SYSTMR, 1000);
    int::set_enabled_interrupts(save);

    loop {}
}
