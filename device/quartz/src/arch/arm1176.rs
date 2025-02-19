pub mod cycle;
pub mod mmu;
pub mod pmm;
pub mod sync;
pub mod tpid;

use core::arch::asm;

#[macro_export]
macro_rules! cpreg {
    ($name:ident, $p:ident, $op1:literal, $crn:ident, $crm:ident, $op2:literal) => {
        pub mod $name {
            $crate::cpreg!(@const $name, $p, $op1, $crn, $crm, $op2);
            $crate::cpreg!(@mut $name, $p, $op1, $crn, $crm, $op2);
        }
    };
    (read $name:ident, $p:ident, $op1:literal, $crn:ident, $crm:ident, $op2:literal) => {
        pub mod $name {
            $crate::cpreg!(@const $name, $p, $op1, $crn, $crm, $op2);
        }
    };
    (write $name:ident, $p:ident, $op1:literal, $crn:ident, $crm:ident, $op2:literal) => {
        pub mod $name {
            $crate::cpreg!(@mut $name, $p, $op1, $crn, $crm, $op2);
        }
    };
    (@const $name:ident, $p:ident, $op1:literal, $crn:ident, $crm:ident, $op2:literal) => {
        #[allow(unused)]
        pub unsafe fn read() -> usize {
            let mut out : usize;
            unsafe { ::core::arch::asm!(
                     concat!("mrc ",stringify!($p),", ",$op1,", {tmp}, ",stringify!($crn),", ",stringify!($crm),", ",$op2),
                     tmp = out(reg) out) };
            out
        }
    };
    (@mut $name:ident, $p:ident, $op1:literal, $crn:ident, $crm:ident, $op2:literal) => {
        #[allow(unused)]
        pub unsafe fn write(arg: usize) {
            unsafe { ::core::arch::asm!(
                     concat!("mcr ",stringify!($p),", ",$op1,", {tmp}, ",stringify!($crn),", ",stringify!($crm),", ",$op2),
                     tmp = in(reg) arg) };
        }
    };
}

cpreg!(write dsb, p15, 0, c7, c10, 4);

pub fn __dsb() {
    unsafe { dsb::write(0) }
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
