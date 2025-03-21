use crate::arch::arm1176::dsb;
use bcm2835_lpa::SYSTMR;
use core::intrinsics::unlikely;
use core::time::Duration;

// we want a version without __dsb()'s for use in tight loops where we have no other peripherals
// (e.g. the delay functions).
unsafe fn __floating_time_unguarded(st: &SYSTMR) -> u64 {
    let hi32 = { st.chi().read().bits() as u64 } << 32;
    hi32 | { st.clo().read().bits() as u64 }
}

/// Read a time in microseconds from the floating system timer. The clock rate is 1MHz.
/// When possible, you should prefer the use of [`Instant`].
pub fn __floating_time(st: &SYSTMR) -> u64 {
    // CHI|CLO runs on a 1MHz oscillator
    dsb();
    let t = unsafe { __floating_time_unguarded(st) };
    dsb();
    t
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Instant {
    floating_micros: u64,
}
impl Instant {
    pub fn now(st: &SYSTMR) -> Self {
        Self {
            floating_micros: __floating_time(st),
        }
    }
    pub fn elapsed(&self, st: &SYSTMR) -> Duration {
        let current_time = __floating_time(st);
        Duration::from_micros(current_time.wrapping_sub(self.floating_micros))
    }
}

/// Blocking wait for (at least) `milliseconds` milliseconds.
/// Implemented on top of [`delay_micros`]; see that function's documentation for timing guarantees.
pub fn delay_millis(st: &SYSTMR, mut milliseconds: u64) {
    const MAX_MILLIS_PER_STEP: u64 = u64::MAX / 1000;
    const SATURATE_TO_MICROS: u64 = MAX_MILLIS_PER_STEP * 1000;
    while milliseconds > MAX_MILLIS_PER_STEP {
        let microseconds = SATURATE_TO_MICROS;
        delay_micros(st, microseconds);
        milliseconds -= MAX_MILLIS_PER_STEP;
    }
    let microseconds = milliseconds * 1000;
    delay_micros(st, microseconds);
}

/// Blocking wait for (at least) `microseconds` microseconds. We can only make a guarantee that it
/// waits for at least `microseconds`, but in practice, in a no-interrupts setting, it should be
/// exact due to the difference in clock rate.
pub fn delay_micros(st: &SYSTMR, microseconds: u64) {
    dsb();
    let start = unsafe { __floating_time_unguarded(st) };
    let end = start.wrapping_add(microseconds);
    if unlikely(end < start) {
        if unlikely(microseconds > (u32::MAX as u64)) {
            // wraparound: end < start <= u64::MAX
            const U64_HALF: u64 = u64::MAX / 2;
            // The first instinct is to just write:
            //  while now >= start || now < end {}
            // however, this breaks down for end == start-1 for micros=u64::MAX
            // so, we check whether we passed 0, and we can therefore use:
            //  while !passed_zero || now < end {}
            // however, this still breaks down for start=u64::MAX, micros=u64::MAX
            // so, we also check whether we passed u64::MAX/2 after passing zero; and once we pass
            // u64::MAX/2, if we go below u64::MAX/2 again, immediately stop the loop.
            let mut passed_zero = false;
            let mut passed_half = false;
            loop {
                let now = unsafe { __floating_time_unguarded(st) };
                if now < start && !passed_zero {
                    passed_zero = true;
                }
                if passed_zero && now >= U64_HALF && !passed_half {
                    passed_half = true;
                }
                if (passed_zero && now >= end) || (passed_half && now < U64_HALF) {
                    break;
                }
            }
        } else {
            // small wraparound 0 <= end << start <= u64::MAX
            while {
                let now = unsafe { __floating_time_unguarded(st) };
                now >= start || now < end
            } {
                // do nothing
            }
        }
    } else {
        // no wraparound: start <= end <= u64::MAX
        while {
            let now = unsafe { __floating_time_unguarded(st) };
            start <= now && now < end
        } {
            // do nothing
        }
    }
    dsb();
}
