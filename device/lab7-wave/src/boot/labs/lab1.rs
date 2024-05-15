use core::arch::{asm, global_asm};
use bcm2835_lpa::Peripherals;
use crate::boot::fmt::Uart1;
use crate::boot::uart1;
use crate::uprintln;
use core::fmt::Write;
use crate::arch::barrier::data_synchronization_barrier;

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

fn part1(uart: &mut Uart1) {
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

extern "C" {
    fn put32(word_ptr: *mut u32, val: u32) -> ();
    fn get32(word_ptr: *mut u32) -> u32;
}

global_asm!(
    "put32:",
    "   str r1, [r0]",
    "   bx lr",
);
global_asm!(
    "get32:",
    "   ldr r0, [r0]",
    "   bx lr",
);

#[no_mangle]
fn test_get32_inline_10() {
    unsafe { asm!("sev") };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { asm!("sev") };
}
#[no_mangle]
fn test_get32_10() {
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
    unsafe { get32(0 as *mut u32) };
}

fn part2(uart: &mut Uart1) {
    // replace put32 with e5801000
    // replace get32 with e5900000
    // sev is e320f004
    let put32_addr = unsafe { core::mem::transmute::<unsafe extern "C" fn(*mut u32, u32) -> (), *const ()>(put32) };
    let get32_addr = unsafe { core::mem::transmute::<unsafe extern "C" fn(*mut u32) -> u32, *const ()>(get32) };
    // bl is
    // COND4:101:L:simm24
    // L=1
    // Target address is calculated by:
    // 1. Sign-extending the 24-bit signed (two's complement) immediate to 30 bits.
    // 2. Shifting the result left two bits to form a 32-bit value.
    // 3. Adding this to the contents of the PC, which contains the address of the branch
    // instruction plus 8 bytes.
    fn decode_bl(word: u32, at: u32) -> Option<*const ()> {
        if (word & 0xf000_0000) == 0xf000_0000 {
            // CC 0b1111, ignore
            return None
        }
        if (word & 0x0f00_0000) != 0x0b00_0000 {
            return None
        }
        let simm24 = word & 0x00ff_ffff;
        let sbit = simm24 & 0x0080_0000;
        // if sbit=0, then (sbit - 1)=0xffff_ffff
        // if sbit=1, then (sbit - 1)=0x007f_ffff
        let sextion = (!(sbit - 1) ) & 0xff00_0000;
        let sext = simm24 | sextion;
        let addr = (sext << 2) + at;
        Some(addr as usize as *const ())
    }

    unsafe fn make_inline(
        p: fn() -> (),
        put32_addr: *const (),
        get32_addr: *const (),
    ) {
        let mut p = unsafe { core::mem::transmute::<fn() -> (), *mut u32>(p) };
        let mut in_body = false;
        // horrifyingly unsafe
        loop {
            let w = *p;
            if in_body {
                if let Some(target) = decode_bl(w, p as usize as u32) {
                    if target == put32_addr {
                        *p = 0xe5801000;
                    } else if target == get32_addr {
                        *p = 0xe5900000;
                    }
                }
            }
            // sev
            if w == 0xe320f004 {
                if !in_body {
                    in_body = true;
                } else {
                    return
                }
            }
            p = p.offset(1);
        }
    }

    // warm the icache
    for _ in 0..4 {
        test_get32_inline_10();
    }

    // timing
    let c0 = cycle_read();
    test_get32_inline_10();
    let c1 = cycle_read();

    unsafe {
        make_inline(test_get32_inline_10, put32_addr, get32_addr);
    }

    // rewarm the icache
    for _ in 0..4 {
        test_get32_inline_10();
    }

    let c2 = cycle_read();
    test_get32_inline_10();
    let c3 = cycle_read();

    uprintln!(uart, "first run: {}", c1-c0);
    uprintln!(uart, "second run: {}", c3-c2);
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn daisy_t1() {
    let uart1= unsafe { Peripherals::steal() }.UART1;
    uprintln!(Uart1::new(&uart1), "t1");
    unsafe { asm!("sev") };
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn daisy_t2() {
    let uart1= unsafe { Peripherals::steal() }.UART1;
    uprintln!(Uart1::new(&uart1), "t2");
    unsafe { asm!("sev") };
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn daisy_t3() {
    let uart1= unsafe { Peripherals::steal() }.UART1;
    uprintln!(Uart1::new(&uart1), "t3");
    unsafe { asm!("sev") };
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn daisy_t4() {
    let uart1= unsafe { Peripherals::steal() }.UART1;
    uprintln!(Uart1::new(&uart1), "t4");
    unsafe { asm!("sev") };
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn daisy_t5() {
    let uart1= unsafe { Peripherals::steal() }.UART1;
    uprintln!(Uart1::new(&uart1), "t5");
    unsafe { asm!("sev") };
}

fn part4(uart: &mut Uart1) {
    // return methods
    // 1: bx lr = e12fff1e
    // COND 00010010 SBO SBO SBO 0001 Rm
    // 2: ldm ???, lr
    // COND 100 P U 0 W 1 Rn register_list

    // daisies don't push lr!!!!

    unsafe fn encode_b(
        to: *const (),
        from: *const (),
    ) -> u32 {
        // b/bl is
        // COND4:101:L:simm24
        // L=1
        // Target address is calculated by:
        // 1. Sign-extending the 24-bit signed (two's complement) immediate to 30 bits.
        // 2. Shifting the result left two bits to form a 32-bit value.
        // 3. Adding this to the contents of the PC, which contains the address of the branch
        // instruction plus 8 bytes.

        let dist = to.byte_offset_from(from.byte_offset(8)) as u32;
        let shift = dist >> 2;
        let bits24 = shift & 0x00ff_ffff;
        let w = 0xea00_0000 | bits24;
        // simm24
        w
    }

    unsafe fn make_hyperret(
        uart: &mut Uart1,
        t: extern "C" fn() -> (),
        n: extern "C" fn() -> (),
        // insert_pop_lr: bool,
    ) {
        let mut p = unsafe { core::mem::transmute::<extern "C" fn() -> (), *mut u32>(t) };
        let mut n = unsafe { core::mem::transmute::<extern "C" fn() -> (), *const ()>(n) };
        // horrifyingly unsafe
        loop {
            let w = *p;

            if w == 0xe12fff1e {
                *p = encode_b(n, p.cast());
                return
            }
            // sev
            // if w == 0xe320f004 {
            //     if *p.offset(1) != 0xe12fff1e {
            //         uprintln!(uart, "sev but NOT bx lr");
            //     } else {
            //         if insert_pop_lr {
            //             *p =
            //         }
            //         p = p.offset(1);
            //     }
            //     return
            // }
            p = p.offset(1);
        }
    }

    for _ in 0..4 {
        daisy_t1();
        daisy_t2();
        daisy_t3();
        daisy_t4();
        daisy_t5();
    }

    let c0 = cycle_read();
    daisy_t1();
    daisy_t2();
    daisy_t3();
    daisy_t4();
    daisy_t5();
    let c1 = cycle_read();

    unsafe { make_hyperret(uart, daisy_t1, daisy_t2) };
    unsafe { make_hyperret(uart, daisy_t2, daisy_t3) };
    unsafe { make_hyperret(uart, daisy_t3, daisy_t4) };
    unsafe { make_hyperret(uart, daisy_t4, daisy_t5) };

    for _ in 0..4 {
        daisy_t1();
    }

    let c2 = cycle_read();
    daisy_t1();
    let c3 = cycle_read();

    uprintln!(uart, "first run: {}", c1-c0);
    uprintln!(uart, "second run: {}", c3-c2);
}

fn part5(uart: &mut Uart1) {
    // partial:
    // given vector A=&[u32], B=&[u32]
    // generate for each nonzero entry a_i
    //  ldr r1, [B], +#4
    // only allow values up to 12 bits
    //  mov r2, #...
    //  mla r3, r2, r1, r3
    // as function:
    //  r0 -> r0
    //  extern "C" fn(*mut u32) -> u32
    // with header:
    // with footer:
    //  mov r0, r3
    unsafe fn gen_partial(a: &[u32], mut out: *mut u32) {
        // ldr <Rd>, [<Rn>], +#n = COND 0100 U B 0 L Rn Rd offset_12 B=0 (word) L=1 (load) U=signof offset
        // ldr <Rd>, [<Rn>, #+/-<offset_12>] = COND 0101 U B 0 L Rn Rd offset_12
        // mov{cond}{S} <Rd>, <shifter_operand> = COND 00 I 1 101 S SBZ Rd shifter_operand
        // mla{<cond>}{S} <Rd>, <Rm>, <Rs>, <Rn> = COND 0000001 S Rd Rn Rs 1001 Rm
        // Rd
        *out = 0xe3a03000;
        out = out.byte_offset(4);
        for (j, a_i) in a.iter().copied().enumerate() {
            if a_i != 0 {
                // // ldr r1, [r0], +#[4*(j-prev_offset)]
                // let p = if j < (a.len() - 1) {
                //     a[j+1..]
                //         .iter()
                //         .position(|&x| x != 0)
                //         .map(|x| (x + 1) * 4)
                // } else {
                //     None
                // };
                // *out = 0xe4901000 | (p.unwrap_or(0) as u32);
                // ldr r1, [r0, #+(j*4)]
                *out = 0xe5901000 | ((4*j) as u32);
                out = out.byte_offset(4);
                // mov r2, #a_i
                *out = 0xe3a02000 | a_i;
                out = out.byte_offset(4);
                // mla r3, r2, r1, r3
                *out = 0xe0233192;
                out = out.byte_offset(4);
            }
        }
        // mov r3, r0
        *out = 0xe1a00003;
        out = out.byte_offset(4);
        *out = 0xe12fff1e;
        out = out.byte_offset(4);
    }

    let a = &[1, 10, 100, 0, 0, 0, 1, 1, 1];
    let b = &[1, 2, 3, 4, 5, 6, 7, 8, 9];
    let thunk = unsafe {
        let alloc = 0x100000 as usize as *mut u32;
        gen_partial(a, alloc);
        // for i in 0..21 {
        //     let p1 = kalloc.offset(i);
        //     uprintln!(uart, "read byte {p1:p} = {:#08x}", *p1);
        // }
        data_synchronization_barrier();
        core::mem::transmute::<*mut u32, extern "C" fn(*const u32) -> u32>(alloc)
    };
    // unsafe { asm!("wfe") };
    // let mut ret : u32 = 0;
    // unsafe {
    //     asm!(
    //         "bl 0x100000",
    //         "mov {rv}, r0",
    //         in("r0") b.as_ptr(),
    //         rv = out(reg) ret,
    //         out("r0") _,
    //         out("r1") _,
    //         out("r2") _,
    //         out("r3") _,
    //     )
    // };
    let ret = thunk(b.as_ptr());
    // unsafe  { asm!("", out("r0") _, out("r1") _, out("r2") _, out("r3") _) };
    // unsafe { asm!("wfe") };
    // let mut s = 0;
    // for j in 0..9 {
    //     let aj = core::hint::black_box(a[j]);
    //     let bj = core::hint::black_box(b[j]);
    //     s += aj * bj;
    // }
    let refv : u32 = a.iter().zip(b.iter()).map(|(i,j)| core::hint::black_box(*i * *j)).sum();
    // unsafe { asm!("wfe") };

    uprintln!(uart, "got result: {ret}");
    uprintln!(uart, "reference: {refv}");

    // let mut r0: u32;
    // let mut r1: u32;
    // let mut r2: u32;
    // let mut r3: u32;
    // let t = unsafe {
    //     asm!(
    //         "mov     r3, #0",
    //         "ldr     r1, [r0]",
    //         "mov     r2, #1",
    //         "mla     r3, r2, r1, r3",
    //         "ldr     r1, [r0, #4]",
    //         "mov     r2, #10",
    //         "mla     r3, r2, r1, r3",
    //         "ldr     r1, [r0, #8]",
    //         "mov     r2, #100",
    //         "mla     r3, r2, r1, r3",
    //         "ldr     r1, [r0, #24]",
    //         "mov     r2, #1",
    //         "mla     r3, r2, r1, r3",
    //         "ldr     r1, [r0, #28]",
    //         "mov     r2, #1",
    //         "mla     r3, r2, r1, r3",
    //         "ldr     r1, [r0, #32]",
    //         "mov     r2, #1",
    //         "mla     r3, r2, r1, r3",
    //         "mov     r0, r3",
    //         // "bx      lr",
    //         // "mov {t0}, #1",
    //         // "mov {t1}, #2",
    //         // "mov {t2}, #10",
    //         // "mla {t3}, {t0}, {t2}, {t1}",
    //         inout("r0") b.as_ptr() => r0,
    //         out("r1") r1,
    //         out("r2") r2,
    //         out("r3") r3,
    //     );
    // };
    // uprintln!(uart, "third ref: r0={r0}, r1={r1}, r2={r2}, r3={r3}");
}

#[inline(always)]
fn cycle_read() -> u32 {
    let mut c: u32;
    unsafe {
        asm!("mrc p15, 0, {t0}, c15, c12, 1", t0 = out(reg) c)
    }
    c
}

#[inline(never)]
pub fn lab1(uart: &mut Uart1) {
    // init cycle counting
    unsafe {
        asm!("mcr p15, 0, {t0}, c15, c12, 0", t0 = in(reg) 1);
    }

    part1(uart);
    part2(uart);
    part4(uart);
    part5(uart);
}