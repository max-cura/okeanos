//! Providence for pointers in rust is tricky.
//! Thus we use inline asm and "addresses" (wink wonk).

use core::arch::asm;

#[inline]
pub unsafe fn read_addr32(addr: usize) -> u32 {
    let mut out : u32;
    #[cfg(target_arch = "arm")]
    asm!(
        "ldr {out}, [{addr}]",
        addr = in(reg) addr,
        out = lateout(reg) out,
    );
    #[cfg(not(target_arch = "arm"))]
    compile_error!("read_addr32 not implemented for your architecture");
    out
}

#[inline]
pub unsafe fn write_addr32(addr: usize, v: u32) {
    #[cfg(target_arch = "arm")]
    asm!(
        "str {v}, [{addr}]",
        addr = in(reg) addr,
        v = in(reg) v,
    );
    #[cfg(not(target_arch = "arm"))]
    compile_error!("write_addr32 not implemented for your architecture");
}