// yanked from `riscv` crate because i for some reason kept getting linking issues if it was in a
// different crate. some modifications.
pub use critical_section::*;
pub struct SingleHartCriticalSection;
set_impl!(SingleHartCriticalSection);

unsafe impl Impl for SingleHartCriticalSection {
    unsafe fn acquire() -> RawRestoreState {
        let mut mstatus: usize;
        unsafe { core::arch::asm!("csrrci {}, mstatus, 0b1000", out(reg) mstatus) };
        let was_active = (mstatus & 0x8) != 0;
        was_active
    }

    unsafe fn release(was_active: RawRestoreState) {
        // Only re-enable interrupts if they were enabled before the critical section.
        if was_active {
            unsafe { core::arch::asm!("csrsi mstatus, 0b1000") };
        }
    }
}
