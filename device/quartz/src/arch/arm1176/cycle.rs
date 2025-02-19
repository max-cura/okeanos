use crate::cpreg;

cpreg!(pmcr, p15, 0, c15, c12, 0);
cpreg!(ccr, p15, 0, c15, c12, 1);

pub fn enable_counters() {
    unsafe {
        pmcr::write(pmcr::read() | 1);
    }
}
pub fn ccr_reset() {
    unsafe { ccr::write(0) };
}
pub fn ccr_read() -> u32 {
    unsafe { ccr::read() as u32 }
}
