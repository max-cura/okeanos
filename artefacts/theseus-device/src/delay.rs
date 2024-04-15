use core::time::Duration;
use bcm2835_lpa::SYSTMR;
use crate::data_synchronization_barrier;

pub fn st_read(st: &SYSTMR) -> u64 {
    // CHI|CLO runs on a 1MHz oscillator
    data_synchronization_barrier();
    let hi32 = {st.chi().read().bits() as u64 } << 32;
    let t = hi32 | { st.clo().read().bits() as u64 };
    data_synchronization_barrier();
    t
}

pub struct STInstant {
    micros: u64
}
impl STInstant {
    pub fn now(st: &SYSTMR) -> Self {
        Self { micros: st_read(st) }
    }
    pub fn elapsed(&self, st: &SYSTMR) -> Duration {
        Duration::from_micros(st_read(st) - self.micros)
    }
}

pub fn delay(st: &SYSTMR, duration: Duration) {
    // yes, we truncate
    let micros = duration.as_micros() as u64;
    delay_micros(st, micros);
}

pub fn delay_micros(st: &SYSTMR, micros: u64) {
    let begin = st_read(st);
    while st_read(st) < (begin + micros) {}
}

pub fn delay_millis(st: &SYSTMR, millis: u32) {
    delay_micros(st, (millis * 1000) as u64);
}