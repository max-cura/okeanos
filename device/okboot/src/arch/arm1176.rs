pub mod mmu;
pub mod pmm;
pub mod sync;

use core::arch::asm;

pub fn __dsb() {
    unsafe {
        asm!(
            // DSB is marked as SBZ, Should Be Zero.
            // See: arm1176.pdf 3-70, 3-71
            "mcr p15,0,{tmp},c7,c10,4",
            tmp = in(reg) 0,
        );
    }
}

pub const PAGE_SIZE: usize = 0x4000;

#[inline]
pub fn __wfe() {
    unsafe { asm!("wfe") }
}

#[inline]
pub fn __sev() {
    unsafe { asm!("sev") }
}
