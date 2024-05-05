use bcm2835_lpa::GPIO;
use lab4_common::arm1176::__dsb;

pub struct Blinken;

impl Blinken {
    pub fn init(gpio: &GPIO) -> Self {
        __dsb();
        gpio.gpfsel0().modify(|_, w| w.fsel6().output().fsel8().output());
        gpio.gpfsel2().modify(|_, w| w.fsel27().output());
        gpio.gpfsel4().modify(|_, w| w.fsel47().output());
        __dsb();
        Self
    }
    // pub fn _5(&self, gpio: &GPIO, x: bool) {
    //     __dsb();
    //     if x {
    //         unsafe { gpio.gpset0().write_with_zero(|w| w.set5().set_bit()) };
    //     } else {
    //         unsafe { gpio.gpclr0().write_with_zero(|w| w.clr5().clear_bit_by_one()) };
    //     }
    //     __dsb();
    // }
    pub fn _6(&self, gpio: &GPIO, x: bool) {
        __dsb();
        if x {
            unsafe { gpio.gpset0().write_with_zero(|w| w.set6().set_bit()) };
        } else {
            unsafe { gpio.gpclr0().write_with_zero(|w| w.clr6().clear_bit_by_one()) };
        }
        __dsb();
    }
    pub fn _8(&self, gpio: &GPIO, x: bool) {
        __dsb();
        if x {
            unsafe { gpio.gpset0().write_with_zero(|w| w.set8().set_bit()) };
        } else {
            unsafe { gpio.gpclr0().write_with_zero(|w| w.clr8().clear_bit_by_one()) };
        }
        __dsb();
    }
    pub fn _27(&self, gpio: &GPIO, x: bool) {
        __dsb();
        if x {
            unsafe { gpio.gpset0().write_with_zero(|w| w.set27().set_bit()) };
        } else {
            unsafe { gpio.gpclr0().write_with_zero(|w| w.clr27().clear_bit_by_one()) };
        }
        __dsb();
    }
    pub fn _47(&self, gpio: &GPIO, x: bool) {
        __dsb();
        if x {
            unsafe { gpio.gpclr1().write_with_zero(|w| w.clr47().clear_bit_by_one()) };
        } else {
            unsafe { gpio.gpset1().write_with_zero(|w| w.set47().set_bit()) };
        }
        __dsb();
    }

    pub fn set(&self, gpio: &GPIO, x: u8) {
        let x1 = x & 1 != 0;
        let x2 = x & 2 != 0;
        let x3 = x & 4 != 0;
        let x4 = x & 8 != 0;
        self._6(gpio, x1);
        self._8(gpio, x2);
        self._27(gpio, x3);
        self._47(gpio, x4);
    }
}
