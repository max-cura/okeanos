use core::arch::asm;

pub mod ptr;
pub mod cpsr;
pub mod mem_barrier;

pub fn nop() {
    unsafe { asm!("nop") }
}