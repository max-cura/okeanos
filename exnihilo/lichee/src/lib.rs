#![allow(internal_features)]
#![feature(core_intrinsics)]
#![no_std]

use core::arch::global_asm;
use core::panic::PanicInfo;

#[repr(C)]
struct Header {
    jump_instruction: u32,
    magic: [u8; 8],
    checksum: u32,
    length: u32,
}
#[unsafe(link_section = ".head")]
#[unsafe(no_mangle)]
static HEADER: Header = Header {
    jump_instruction: 0,
    magic: *b"eGON.BT0",
    checksum: 0x5F0A6C39,
    length: 0,
};

unsafe extern "C" {
    pub static __symbol_stack_end__: [u8; 0];
    pub static __symbol_bss_start__: [u8; 0];
    pub static __symbol_bss_end__: [u8; 0];
}

global_asm!(
    r#"
.globl _start
.section ".start"
.extern {START}
_start:
    csrw mie, zero
    li t1, {EN_THEADISAEE}
    csrs 0x7c0, t1
    li t2, 0x30013
    csrs 0x7c2, t2
    la sp, {STACK_HIGH}
    jal _clear_bss
    jal {START}
	j .
_clear_bss:
    la t0, {BSS_START}
    la t1, {BSS_END}
.L0:
    sw zero, 0(t0)
    addi t0, t0, 8
    blt t0, t1, .L0
    ret
"#,
    EN_THEADISAEE = const 0x1 << 22,
    STACK_HIGH = sym __symbol_stack_end__,
    START = sym __symbol_kstart,
    BSS_START = sym __symbol_bss_start__,
    BSS_END = sym __symbol_bss_end__,
);

#[unsafe(no_mangle)]
pub extern "C" fn __symbol_kstart() -> ! {
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
