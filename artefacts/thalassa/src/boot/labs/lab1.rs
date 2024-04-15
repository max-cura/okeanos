use core::arch::asm;
use bcm2835_lpa::Peripherals;
use crate::boot::fmt::Uart1;
use crate::boot::uart1;
use crate::uprintln;
use core::fmt::Write;

#[no_mangle]
// r0=*UART1 s=*str l=strlen
unsafe extern "C" fn _stub_print_str(s: *const u8, l: usize) {
    let uart1= unsafe { Peripherals::steal() }.UART1;

    uprintln!(Uart1::new(&uart1), "called with s={s:#?} l={l:#?}");
    // SAFETY: ... it was nice knowing you
    uart1::uart1_write_bytes(&uart1, unsafe {
        core::str::from_raw_parts(s, l)
    }.as_bytes());
    uprintln!(Uart1::new(&uart1), "done writing");
}

#[inline(never)]
pub fn lab1(uart: &mut Uart1) {
    // stub1();
    let mut data: [u32 ; 11] = [
        0xe92d4800, //push	{fp, lr}
        0xe59f200c, //ldr	r2, [pc, #12]
        0xe28f000c, //add	r0, pc, #12
        0xe3a0100e, //mov	r1, #14
        0xe12fff32, //blx	r2
        0xe8bd8800, //pop	{fp, pc}

        0x00000000, // <- _stub_print_str goes here

        0x6c6c6548, // lleH
        0x77202c6f, // w ,o
        0x646c726f, // dlro
        0x00000a21, // ...!
    ];
    let addr = &data as *const u32 as *const u8;
    let f = _stub_print_str;

    unsafe {
        data[6] = core::mem::transmute::<unsafe extern "C" fn(*const u8, usize) -> (), *const u32>(f) as usize as u32;
    }
    for (i, w) in data.iter().enumerate() {
        uprintln!(uart, "word at {:#?} is {w:#x}", &data[i] as *const u32);
    }
    uprintln!(uart, "wrote to address {addr:#?}; now jumping");

    unsafe {
        asm!(
            "blx {addr}",
            "nop",
            addr = in(reg) addr,
            out("r2") _,
            out("r1") _,
            out("r0") _,
            clobber_abi("C")
        );
    }

    uprintln!(uart, "jump over, returned");
}