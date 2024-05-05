use core::arch::asm;
use bcm2835_lpa::Peripherals;
use crate::boot::fmt::Uart1;
use crate::uprintln;
use core::fmt::Write;
use crate::arch::barrier::data_synchronization_barrier;
use crate::timing::{__floating_time, delay_micros};

#[inline(always)]
fn cycle_read() -> u32 {
    let mut c: u32;
    unsafe {
        asm!("mrc p15, 0, {t0}, c15, c12, 1", t0 = out(reg) c)
    }
    c
}

extern "C" {
    static _lab6_ivec: [u32; 8];
}

pub fn lab6(uart: &mut Uart1) {
    uprintln!(uart, "starting waveform generator");

    use data_synchronization_barrier as __dsb;

    let peri = unsafe { Peripherals::steal() };

    let gpio = &peri.GPIO;
    let cm_pwm = &peri.CM_PWM;
    let st = &peri.SYSTMR;
    let pwm = &peri.PWM0;

    __dsb();
    gpio.gpfsel0().modify(|_, w| w.fsel4().gpclk0());
    gpio.gpfsel1().modify(|_, w| w.fsel12().pwm0_0());
    __dsb();

    let cm_gp0ctl : *mut u32 = 0x20101070 as _;
    let cm_gp0div : *mut u32 = 0x20101074 as _;

    unsafe {
        // PLLC (? designation unclear) 1GHz
        cm_gp0ctl.write_volatile(0x5a00_0005);
    }
    delay_micros(st, 110000);
    while {
        (unsafe { cm_gp0ctl.read_volatile() } & 0x80) != 0
    } {}

    let divi = 1000;
    let divf = 0;
    let v = divf | (divi << 12) | 0x5a00_0000;
    unsafe {
        cm_gp0div.write_volatile(v)
    }
    unsafe {
        cm_gp0ctl.write_volatile(0x5a00_0015);
    }
}

#[inline(never)]
pub fn lab6_scope(uart: &mut Uart1) {
    uprintln!(uart, "state ok");

    // init cycle counting
    unsafe {
        asm!("mcr p15, 0, {t0}, c15, c12, 0", t0 = in(reg) 1);
    }

    let peri = unsafe { Peripherals::steal() };

    let p_buf = 0x00100000_usize as *mut u32;
    let p_buf_sz = 0x00100000_isize; // 1MB
    let p_buf_end = unsafe { p_buf.byte_offset(p_buf_sz) };
    let p_gpio_base = 0x2020_0000_usize as *mut u32;

    use data_synchronization_barrier as __dsb;
    let gpio = &peri.GPIO;

    let t0 = __floating_time(&peri.SYSTMR);
    // ~800us
    for _ in 0..10000 {
        unsafe { asm!("nop") }
    }
    let t1 = __floating_time(&peri.SYSTMR);

    uprintln!(uart, "nop4096 = {}", t1-t0);

    __dsb();
    unsafe { gpio.gpclr0().write_with_zero(|w| w.clr27().clear_bit_by_one()) }
    __dsb();

    // uprintln!(uart, "waiting...");
    // delay_millis(&peri.SYSTMR, 1000);
    uprintln!(uart, "starting sample");

    unsafe {
        __dsb();
        gpio.gpfsel1().modify(|_, w| {
            w.fsel13().input()
        });
        gpio.gpfsel2().modify(|_, w| w.fsel25().output().fsel26().output());
        gpio.gpafen0().modify(|_, w| w.afen13().set_bit());
        gpio.gparen0().modify(|_, w| w.aren13().set_bit());
        gpio.gplen0().modify(|_, w| w.len13().clear_bit());
        gpio.gphen0().modify(|_, w| w.hen13().clear_bit());
        gpio.gpren0().modify(|_, w| w.ren13().clear_bit());
        gpio.gpfen0().modify(|_, w| w.fen13().clear_bit());
        unsafe { gpio.gpeds0().write_with_zero(|w| w.eds13().clear_bit_by_one()); }
        __dsb();

        // set non-SMON vector table base address
        asm!("mcr p15, 0, {vec}, c12, c0, 0", vec=in(reg) core::ptr::addr_of!(_lab6_ivec));
        // disable interrupts
        let irq_dis1_reg = 0x2000b21c_usize as *mut u32;
        let irq_dis2_reg = 0x2000b220_usize as *mut u32;
        irq_dis1_reg.write_volatile(0xffff_ffff);
        irq_dis2_reg.write_volatile(0xffff_ffff);
        // set FIQ mode
        let fiq_ctl_reg = 0x2000b20c_usize as *mut u32;
        let f0 = fiq_ctl_reg.read_volatile();
        uprintln!(uart, "f0={f0:#08x}");
        let f1 = (f0 & 0xffff_ff00) | 0x80 | 49;
        uprintln!(uart, "f1={f1:#08x}");
        fiq_ctl_reg.write_volatile(f1);
        let f2 = fiq_ctl_reg.read_volatile();
        uprintln!(uart, "f2={f2:#08x}");

        __dsb();

        // set V=0 (high interrupt tables=0), bit 13
        // (dont) set FI=1 (low interrupt latency mode), bit 21
        // FI destabilizes timings?
        asm!(
            "mrc p15, 0, {t0}, c1, c0, 0",
            "and {t0}, {t0}, #(~(1<<13))",
            // "orr {t0}, {t0}, #(1 << 21)",
            "mcr p15, 0, {t0}, c1, c0, 0",
            t0 = out(reg) _,
        );

        __dsb();

        // set FIQ state
        asm!(
            "mrs {t0}, cpsr",
            "and {t0}, {t0}, #(~0b11111)",
            "orr {t0}, {t0}, #(0b10001)", // FIQ
            "msr cpsr, {t0}",
            // r12_fiq=buffer end
            // r11_fiq=gpio base
            "mov r13, {p_buf}",
            "mov r12, {p_buf_end}",
            "mov r11, {p_gpio}",

            "mrs {t0}, cpsr",
            "and {t0}, {t0}, #(~0b11111)",
            "orr {t0}, {t0}, #(0b10011)", // SUPER
            "msr cpsr, {t0}",
            t0 = out(reg) _,
            p_buf = in(reg) p_buf,
            p_buf_end = in(reg) p_buf_end,
            p_gpio = in(reg) p_gpio_base,
        );
        // set URWTPIDR
        asm!(
            "mcr p15, 0, {t0}, c13, c0, 2",
            t0=in(reg) p_buf
        );
        // enable FIQ
        asm!(
            "mrs {t0}, cpsr",
            "and {t0}, {t0}, #(~(1<<6))",
            "msr cpsr, {t0}",
            t0 = out(reg) _,
        );
    }

    // 1ms
    // delay_micros(&peri.SYSTMR, 1000);
    // ~800us
    // loop uses r0; we're really just not returning the right value
    // unsafe {
    //     asm!(
    //         ".align 4",
    //         "2:",
    //         "nop",
    //         // "wfe",
    //         "subs {c}, {c}, #1",
    //         "bne 2b",
    //         c = in(reg) 10000,
    //     )
    // }
    for _ in 0..10000 {
        unsafe {
            // asm!("wfe")
            asm!("nop");
        }
    }
    // 700MHz, nop is something like 80 cy?

    unsafe {
        asm!(
            "mrs {t0}, cpsr",
            "orr {t0}, {t0}, #(1<<6)",
            "msr cpsr, {t0}",
            t0 = out(reg) _,
        )
    }

    uprintln!(uart, "done sampling");

    let stack_end = unsafe {
        let mut t0 : u32 = 0;
        asm!(
            "mrc p15, 0, {t0}, c13, c0, 2",
            t0 = out(reg) t0
        );
        t0 as usize as *const u32
    };
    uprintln!(uart, "stack_end={stack_end:p} stack_begin={p_buf:p}");

    let buf = unsafe {
        let stack_end_ptr = stack_end;
        core::slice::from_raw_parts(
            p_buf,
            stack_end_ptr.offset_from(p_buf) as usize
        )
    };
    #[derive(Debug, Copy, Clone)]
    #[repr(C)]
    struct Record {
        cyc: u32,
        mask: u32,
        lev: u32,
        cyc_dbg: u32,
    }
    unsafe impl bytemuck::Zeroable for Record {}
    unsafe impl bytemuck::Pod for Record {}
    uprintln!(uart, "buf.len() = {}", buf.len());
    assert_eq!(buf.len() % 4, 0);
    let records : &[Record] = bytemuck::cast_slice(buf);
    for i in 0..records.len()-1 {
        let l = records[i];
        let r = records[i+1];
        let length = r.cyc.wrapping_sub(l.cyc);
        let fiq_time = l.cyc_dbg.wrapping_sub(l.cyc);
        let mask_high = l.mask & l.lev;
        let mask_low = l.mask & !l.lev;
        uprintln!(uart, "record #{i}: length={length}cy mask_hi={mask_high} mask_lo={mask_low}, processing time={fiq_time}cy");
    }
}
