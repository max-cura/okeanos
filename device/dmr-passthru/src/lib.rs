#![feature(panic_info_message)]
#![feature(error_in_core)]
#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(try_blocks)]
#![feature(thread_local)]
#![feature(pointer_is_aligned_to)]
#![feature(slice_ptr_get)]
#![feature(array_ptr_get)]
#![feature(format_args_nl)]
#![feature(allocator_api)]
#![feature(iter_repeat_n)]
#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use bcm2835_lpa::Peripherals;
use bismuth::arch::arm1176::__dsb;
use bismuth::arch::arm1176::pmm::RegionKind;
use bismuth::boot::PMM;
use bismuth::data::circular_buffer::CircularBuffer;
use bismuth::{uart1_sendln_bl, MiB};
use core::alloc::GlobalAlloc;
use core::arch::asm;
use embedded_alloc::Heap;

#[global_allocator]
static HEAP: Heap = Heap::empty();

extern "C" {
    static __bis__lic_table: [u32; 8];
}

#[no_mangle]
pub extern "C" fn __bis__main() {
    uart1_sendln_bl!("=== RPI-DOWNLOADMOARRAM PASSTHROUGH ===");
    let peri = unsafe { Peripherals::steal() };

    {
        const HEAP_SIZE: usize = 16 * MiB;
        let heap_mem = (&mut PMM.get().lock())
            .allocate_region(RegionKind::Supersection)
            .unwrap();
        unsafe {
            HEAP.init(heap_mem as usize, HEAP_SIZE);
        }
    }

    __dsb();
    unsafe {
        peri.GPIO.gpfsel0().modify(|_, w| {
            w.fsel0()
                .input()
                .fsel1()
                .input()
                .fsel2()
                .input()
                .fsel3()
                .input()
                .fsel4()
                .input()
                .fsel5()
                .input()
                .fsel6()
                .input()
                .fsel7()
                .input()
        });
        peri.GPIO.gpfsel1().modify(|_, w| {
            w.fsel16()
                .input()
                .fsel17()
                .input()
                .fsel18()
                .input()
                .fsel19()
                .input()
        });
        peri.GPIO.gpfsel2().modify(|_, w| {
            w.fsel20()
                .input()
                .fsel21()
                .input()
                .fsel22()
                .input()
                .fsel23()
                .input()
                .fsel27()
                .output()
        });
        peri.GPIO.gpfen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gpren0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gpafen0().write_with_zero(|w| w.bits(0x0000_00c0));
        peri.GPIO.gparen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gplen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gphen0().write_with_zero(|w| w.bits(0x0000_0000));
    }
    __dsb();

    let stack_box = vec![0u8; 0x1000].into_boxed_slice();
    let stack = stack_box.as_ptr();
    let _ = Box::into_raw(stack_box);
    let p_uart_base = 0x20215040_usize as *mut u32;
    let p_gpio_base = 0x2020_0000_usize as *mut u32;

    unsafe {
        asm!("mcr p15, 0, {vec}, c12, c0, 0", vec=in(reg) core::ptr::addr_of!(__bis__lic_table));
    }

    unsafe {
        // disable interrupts
        let irq_dis1_reg = 0x2000b21c_usize as *mut u32;
        let irq_dis2_reg = 0x2000b220_usize as *mut u32;
        irq_dis1_reg.write_volatile(0xffff_ffff);
        irq_dis2_reg.write_volatile(0xffff_ffff);
        // set FIQ mode
        let fiq_ctl_reg = 0x2000b20c_usize as *mut u32;
        let f0 = fiq_ctl_reg.read_volatile();
        uart1_sendln_bl!("f0={f0:#08x}");
        let f1 = (f0 & 0xffff_ff00) | 0x80 | 49;
        uart1_sendln_bl!("f1={f1:#08x}");
        fiq_ctl_reg.write_volatile(f1);
        let f2 = fiq_ctl_reg.read_volatile();
        uart1_sendln_bl!("f2={f2:#08x}");
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
        "mov r13, {stack}",
        "mov r12, {p_uart_base}",
        "mov r11, {p_gpio}",

        "mrs {t0}, cpsr",
        "and {t0}, {t0}, #(~0b11111)",
        "orr {t0}, {t0}, #(0b10011)", // SUPER
        "msr cpsr, {t0}",
        t0 = out(reg) _,
        stack = in(reg) stack,
        p_uart_base = in(reg) p_uart_base,
        p_gpio = in(reg) p_gpio_base,
        );
        // // set URWTPIDR
        // asm!(
        // "mcr p15, 0, {t0}, c13, c0, 2",
        // t0=in(reg) p_buf
        // );
        // enable FIQ
        asm!(
        "mrs {t0}, cpsr",
        "and {t0}, {t0}, #(~(1<<6))",
        "msr cpsr, {t0}",
        t0 = out(reg) _,
        );
    }

    unsafe {
        peri.GPIO.gpeds0().write_with_zero(|w| w.bits(0xffff_ffff));
    }

    uart1_sendln_bl!("started listening...");

    loop {}
    // loop {
    //     __dsb();
    //     let eds = peri.GPIO.gpeds0().read().bits();
    //     if eds & 0xc0 != 0 {
    //         unsafe {
    //             asm!("nop", "nop", "nop", "nop");
    //         }
    //         let lev = peri.GPIO.gplev0().read().bits();
    //         unsafe {
    //             peri.GPIO.gpeds0().write_with_zero(|w| w.bits(0xffff_ffff));
    //         }
    //         if lev & 0x3f == 0 {
    //             let byte = ((lev & 0x00ff_0000) >> 16) as u8;
    //             __dsb();
    //             unsafe {
    //                 // write
    //             }
    //         }
    //     }
    // }

    // struct LineBufferedPassthrough {
    //     buf: CircularBuffer,
    //     newline_count: usize,
    //     byte: u8,
    //     status: bool,
    // }
    // impl LineBufferedPassthrough {
    //     pub fn new() -> Self {
    //         let buf = CircularBuffer::new(vec![0; 0x100000].leak());
    //         Self {
    //             buf,
    //             newline_count: 0,
    //             byte: 0,
    //             status: false,
    //         }
    //     }
    //     pub fn push(&mut self, n: u8) {
    //         if self.status {
    //             let b = (self.byte << 4) | n;
    //             self.buf.push_byte(b);
    //             if b == b'\n' {
    //                 self.newline_count += 1;
    //             }
    //             self.status = false;
    //         } else {
    //             self.byte = n;
    //             self.status = true;
    //         }
    //     }
    // }
    // fn poll(gpio: &GPIO, left: &mut LineBufferedPassthrough, right: &mut LineBufferedPassthrough) {
    //     __dsb();
    //     let eds = gpio.gpeds0().read();
    //     if eds.eds24().bit_is_set() || eds.eds25().bit_is_set() {
    //         unsafe {
    //             gpio.gpeds0()
    //                 .write_with_zero(|w| w.eds24().clear_bit_by_one().eds25().clear_bit_by_one())
    //         }
    //         let lev = gpio.gplev0().read().bits();
    //         if eds.eds24().bit_is_set() {
    //             let nybble = (lev >> 18) & 0x0f;
    //             left.push(nybble as u8);
    //         }
    //         if eds.eds25().bit_is_set() {
    //             let nybble = (lev >> 8) & 0x0f;
    //             right.push(nybble as u8);
    //         }
    //     }
    //     __dsb();
    // }

    // let mut left = LineBufferedPassthrough::new();
    // let mut right = LineBufferedPassthrough::new();
    // let mut which = 0;
    // loop {
    //     poll(&peri.GPIO, &mut left, &mut right);
    //     __dsb();
    //     let lsr = peri.UART1.lsr().read();
    //     if lsr.tx_empty().bit_is_set() {
    //         if which == 3 {
    //             if let Some(b) = left.buf.shift_byte() {
    //                 unsafe { peri.UART1.io().write_with_zero(|w| w.data().variant(b)) }
    //                 if b == b'\n' {
    //                     left.newline_count -= 1;
    //                     which = 2;
    //                 }
    //             }
    //         } else if which == 4 {
    //             if let Some(b) = right.buf.shift_byte() {
    //                 unsafe { peri.UART1.io().write_with_zero(|w| w.data().variant(b)) }
    //                 if b == b'\n' {
    //                     right.newline_count -= 1;
    //                     which = 1;
    //                 }
    //             }
    //         } else {
    //             let (lav, rav) = (left.newline_count > 0, right.newline_count > 0);
    //             if lav && rav {
    //                 which = if which == 1 || which == 0 {
    //                     3 // left
    //                 } else {
    //                     4 // right
    //                 };
    //             } else {
    //                 if lav {
    //                     which = 3
    //                 } else if rav {
    //                     which = 4
    //                 }
    //             }
    //
    //             if which == 3 {
    //                 unsafe { peri.UART1.io().write_with_zero(|w| w.data().variant(b'>')) }
    //             } else if which == 4 {
    //                 unsafe { peri.UART1.io().write_with_zero(|w| w.data().variant(b'<')) }
    //             }
    //         }
    //     }
    //     __dsb();
    // }
}

#[no_mangle]
pub extern "C" fn __aeabi_unwind_cpp_pr0() {}
