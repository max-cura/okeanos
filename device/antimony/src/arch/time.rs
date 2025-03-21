use core::arch::asm;
use core::ops::Sub;
use core::time::Duration;

/// Returns with a resolution somewhere in the vicinity of ~42~45ns.
/// According to [this](https://misc0110.net/web/files/riscv_attacks_sp23.pdf), it's exactly 45ns.
/// I measured against UART at ~115200 baud, and got ~42ns, so I figure ~45ns is probably right if
/// we ignore overhead;
pub fn now_raw() -> u64 {
    let mut out: u64;
    unsafe {
        asm!("rdtime {t}", t = out(reg) out);
    }
    out
}

#[derive(Debug, Copy, Clone)]
pub struct Instant(u64);

pub const fn never() -> Instant {
    Instant(0)
}

pub fn now() -> Instant {
    Instant(now_raw())
}

impl Sub<Instant> for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Self::Output {
        let diff = self.0.wrapping_sub(rhs.0);
        Duration::from_nanos(diff * 45)
    }
}

pub fn delay(millis: u64) {
    let start = now();
    while (now() - start) > Duration::from_millis(millis) {}
}

pub fn delay_micros(micros: u64) {
    let start = now();
    while (now() - start) > Duration::from_micros(micros) {}
}
