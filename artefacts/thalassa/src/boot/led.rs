use bcm2835_lpa::GPIO;
use crate::arch::barrier::data_synchronization_barrier;

pub enum Logic {
    High,
    Low,
}

pub fn led_init(gpio: &GPIO) {
    data_synchronization_barrier();
    gpio.gpfsel4().modify(|_, w| {
        w.fsel47().output()
    });
    data_synchronization_barrier();
}

pub fn led_set(gpio: &GPIO, state: Logic) {
    data_synchronization_barrier();
    match state {
        Logic::High => {
            unsafe { gpio.gpset1().write_with_zero(|w| w.set47().set_bit()); }
        }
        Logic::Low => {
            unsafe { gpio.gpclr1().write_with_zero(|w| w.clr47().clear_bit_by_one()); }
        }
    }
    data_synchronization_barrier();
}