use crate::ir::PDC_BIT_DURATION;
use core::time::Duration;

#[derive(Debug, Copy, Clone)]
pub struct RateRelativeTimeout {
    bytes: usize,
}
impl RateRelativeTimeout {
    pub const fn from_bytes(n: usize) -> Self {
        Self { bytes: n }
    }

    pub const fn with_ir(self) -> Duration {
        use super::ir::PDC_BIT_DURATION;

        Duration::from_micros(PDC_BIT_DURATION.as_micros() as u64 * (self.bytes as u64) * 8)
    }

    pub const fn at_baud_8n1(self, baud: u32) -> Duration {
        // at 8n1, we have flat 80% efficiency; then we have 1 byte/10 bits
        // so byte_rate = baud/10 B/s
        // so time = bytes / byte_rate
        // problem: byte_rate much higher than bytes; up to 3.125 MB/s
        // we don't have floats (yet), so we get a bit awkward, since we're in units of
        // microseconds; thus, we use fixed point on 10^6 and round up

        // BUGFIX: multiply overflow!
        //  using u32 math, 0x4000.at_baud_8n1(115200)~=300ms
        //                 0x10000.at_baud_8n1(115200)~=96ms
        //  thus we now use u64 instead

        let byte_rate = baud as u64 / 10;

        let bytes_mega = self.bytes as u64 * 1_000_000;
        let microseconds = (bytes_mega + byte_rate - 1) / byte_rate;

        Duration::from_micros(microseconds)
    }
}
