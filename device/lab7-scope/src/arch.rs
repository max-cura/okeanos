use core::arch::asm;

pub mod ptr;
pub mod cpsr;
pub mod barrier;
//pub mod spin_lock;

pub fn nop() {
    unsafe { asm!("nop") }
}