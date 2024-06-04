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

mod request;

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use bcm2835_lpa::{Peripherals, CM_PWM, PWM0, SYSTMR};
use bismuth::arch::arm1176::__dsb;
use bismuth::arch::arm1176::pmm::{RegionKind, PMM};
use bismuth::arch::arm1176::timing::{cycle_init, cycle_read, delay_micros, delay_millis, Instant};
use bismuth::boot::PMM;
use bismuth::peripherals::dma::{DMA, DMA_CB, DMA_CS, DMA_TI};
use bismuth::peripherals::smi::{
    smi_init, SMIConfig, SMIDataWidth, CM_CTL, CM_DIV, CM_SMI, SMI, SMI_A, SMI_CS, SMI_DA, SMI_DCS,
    SMI_DSR, SMI_DSW,
};
use bismuth::{uart1_sendln_bl, MiB};
use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use embedded_alloc::Heap;
use proc_bitfield::WithBit;
use volatile_register::{RO, RW};

#[global_allocator]
static HEAP: Heap = Heap::empty();

#[no_mangle]
pub extern "C" fn __bis__main() {
    uart1_sendln_bl!("=== RPI-DOWNLOADMOARRAM CLIENT ===");
    // GLOBAL_ALLOC.0.get_or_init(|| {
    //     SimpleAlloc::new(
    //         (&mut PMM.get().lock())
    //             .allocate_region(RegionKind::Supersection)
    //             .unwrap(),
    //         16 * MiB,
    //     )
    // });
    {
        const HEAP_SIZE: usize = 16 * MiB;
        let heap_mem = (&mut PMM.get().lock())
            .allocate_region(RegionKind::Supersection)
            .unwrap();
        unsafe {
            HEAP.init(heap_mem as usize, HEAP_SIZE);
        }
    }
    let peri = unsafe { Peripherals::steal() };

    let st = &peri.SYSTMR;

    __dsb();
    unsafe {
        peri.GPIO.gpfsel0().modify(|_, w| {
            w.fsel0()
                .sa5()
                .fsel1()
                .sa4()
                .fsel2()
                .sa3()
                .fsel3()
                .sa2()
                .fsel4()
                .sa1()
                .fsel5()
                .sa0()
                .fsel6()
                .soe_n()
                .fsel7()
                .swe_n()
                .fsel8()
                .sd0()
                .fsel9()
                .sd1()
        });
        peri.GPIO.gpfsel1().modify(|_, w| {
            w.fsel10()
                .sd2()
                .fsel11()
                .sd3()
                .fsel12()
                .sd4()
                .fsel13()
                .sd5()
                // .fsel14()
                // .sd6()
                // .fsel15()
                // .sd7()
                .fsel16()
                .sd8()
                .fsel17()
                .sd9()
                .fsel18()
                .sd10()
                .fsel19()
                .sd11()
        });
        peri.GPIO.gpfsel2().modify(|_, w| {
            w.fsel20()
                .sd12()
                .fsel21()
                .sd13()
                .fsel22()
                .sd14()
                .fsel23()
                .sd15()
                .fsel24()
                .output()
                // .sd16()
                .fsel25()
                .output()
                // .sd17()
                .fsel26()
                .input()
                .fsel27()
                .output()
        });
        peri.GPIO.gpfen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gpren0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gpafen0().write_with_zero(|w| w.bits(0x0400_0000));
        peri.GPIO.gparen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gplen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gphen0().write_with_zero(|w| w.bits(0x0000_0000));
    }
    __dsb();

    uart1_sendln_bl!("selected GPIO ALTs");

    let cm_smi = unsafe { CM_SMI::steal() };
    let smi = unsafe { SMI::steal() };
    let dma = unsafe { DMA::steal() };

    __dsb();

    cycle_init();

    request::dmr_init(&smi, &cm_smi, &dma, st);

    uart1_sendln_bl!("SMI STATE: ");
    uart1_sendln_bl!("SMI_CS={:?}", smi.cs.read());
    uart1_sendln_bl!("SMI_DC={:?}", smi.dc.read());

    let mut recv_buf = [0u32; 1024];
    let c1 = cycle_read();
    request::dmr_issue_read_command(
        0x10002000,
        RegionKind::SmallPage,
        recv_buf.as_mut_ptr(),
        &smi,
        &dma,
        &peri.GPIO,
        0b000001,
    );
    let c2 = cycle_read();
    uart1_sendln_bl!("=== transfer (1) finished ({}) ===", c2 - c1);
    for i in 0..8 {
        let word = unsafe { recv_buf.as_mut_ptr().offset(i).read_volatile() };
        uart1_sendln_bl!("word at recv_buf+{i} is {word:08x}");
    }

    uart1_sendln_bl!("SMI STATE: ");
    uart1_sendln_bl!("SMI_CS={:?}", smi.cs.read());
    uart1_sendln_bl!("SMI_DC={:?}", smi.dc.read());

    unsafe {
        peri.GPIO
            .gpclr0()
            .write_with_zero(|w| w.clr27().clear_bit_by_one())
    }
    delay_millis(st, 100);

    uart1_sendln_bl!("starting second transaction...");

    let mut recv_buf = [0u32; 1024];
    let c1 = cycle_read();
    request::dmr_issue_read_command(
        0x10002000,
        RegionKind::SmallPage,
        recv_buf.as_mut_ptr(),
        &smi,
        &dma,
        &peri.GPIO,
        0b000001,
    );
    let c2 = cycle_read();
    uart1_sendln_bl!("=== transfer (1) finished ({}) ===", c2 - c1);
    for i in 0..8 {
        let word = unsafe { recv_buf.as_mut_ptr().offset(i).read_volatile() };
        uart1_sendln_bl!("word at recv_buf+{i} is {word:08x}");
    }

    // let mut send_buf = [0u32; 1024];
    // for i in 0..1024 {
    //     send_buf[i] = i as u32 * 0x0001_0001;
    // }
    // let c1 = cycle_read();
    // request::dmr_issue_write_command(
    //     0x10001000,
    //     RegionKind::SmallPage,
    //     send_buf.as_mut_ptr(),
    //     &smi,
    //     &dma,
    //     &peri.GPIO,
    //     0b000001,
    // );
    // let c2 = cycle_read();
    // uart1_sendln_bl!("=== transfer (2) finished ({}) ===", c2 - c1);
    // for i in 0..8 {
    //     let word = unsafe { send_buf.as_mut_ptr().offset(i).read_volatile() };
    //     uart1_sendln_bl!("word at send_buf+{i} is {word:08x}");
    // }

    // __dsb();
    //
    // smi_init(
    //     st,
    //     &cm_smi,
    //     &smi,
    //     SMIConfig {
    //         width: SMIDataWidth::Bits16, // 1 us
    //         clock_ns: 1000,              // 1us
    //         setup_cycles: 15,
    //         strobe_cycles: 40,
    //         hold_cycles: 15,
    //         pace_cycles: 0,
    //     },
    // );
    //
    // fn mem_bus_addr(p: u32) -> u32 {
    //     // FUCK YOU BROADCOM
    //     // I WASTED 5 HOURS BECAUSE YOU GAVE ME THE WRONG GODDAMN VALUE
    //     0x4000_0000 + p
    // }
    // fn reg_bus_addr(p: u32) -> u32 {
    //     0x7e00_0000 + (p - 0x2000_0000)
    // }
    //
    // unsafe {
    //     // peri.GPIO.gpset0().write_with_zero(|w| w.set24().set_bit());
    //
    //     __dsb();
    //
    //     // uart1_sendln_bl!("setting up for DMA transfer");
    //
    //     smi.a.modify(|r| r.with_address(0b0000_01));
    //     smi.da.modify(|r| r.with_address(0b0000_01));
    //
    //     smi.dc.modify(|r| r.with_dmap(true));
    //     // smi.devices[0].dsw.modify(|r| r.with_wdreq(true));
    //
    //     // smi.devices[0].dsw.modify(|r| r.with_wdreq(true));
    //
    //     smi.cs.modify(|r| r.with_pxldat(true));
    //     smi.cs.modify(|r| r.with_clear(true));
    //     smi.cs.modify(|r| r.with_clear(true));
    //     smi.cs.modify(|r| r.with_seterr(true));
    //     smi.cs.modify(|r| r.with_aferr(true));
    //     smi.dcs.modify(|r| r.with_enable(true));
    //     smi.dc
    //         .modify(|r| r.with_reqr(2).with_reqw(2).with_panicr(8).with_panicw(8));
    //     // 8bit?
    //     // smi.devices[0].dsr.modify(|r| r.with_rwidth(0));
    //     // 0x110 transfers =
    //     // smi.l.write(0x110);
    //
    //     static PAYLOAD: &[u32] = &[
    //         0x80000000 // write
    //             | 0x10001000 // address
    //             | 0x00000000, // small page,
    //     ];
    //     // static PAYLOAD: &[u32] = &[
    //     //     0x00000000 // read
    //     //         | 0x10002000 // address
    //     //         | 0x00000000, // small page,
    //     // ];
    //
    //     let mut test_buf = [0u32; 1024];
    //     for i in 0..1024 {
    //         test_buf[i] = (i as u32) << 16 | (i as u32);
    //     }
    //
    //     let mut cbs = vec![
    //         DMA_CB {
    //             ti: DMA_TI(0)
    //                 .with_dest_dreq(true)
    //                 .with_permap(4)
    //                 .with_src_inc(true),
    //             // .with_dest_inc(true),
    //             srce_ad: mem_bus_addr(PAYLOAD.as_ptr() as usize as u32),
    //             // 0x2060_0000, 4th field so 0x2060_000c
    //             dest_ad: reg_bus_addr(0x2060_000c),
    //             tfr_len: 4,
    //             stride: 0,
    //             next_cb: 0,
    //             debug: 0,
    //             _unused: 0,
    //         },
    //         DMA_CB {
    //             ti: DMA_TI(0)
    //                 .with_dest_dreq(true)
    //                 .with_permap(4)
    //                 .with_src_inc(true),
    //             srce_ad: mem_bus_addr(test_buf.as_mut_ptr() as usize as u32),
    //             dest_ad: reg_bus_addr(0x2060_000c),
    //             tfr_len: 0x1000,
    //             stride: 0,
    //             next_cb: 0,
    //             debug: 0,
    //             _unused: 0,
    //         },
    //         DMA_CB {
    //             ti: DMA_TI(0)
    //                 .with_src_dreq(true)
    //                 .with_dest_inc(true)
    //                 .with_permap(4)
    //                 .with_wait_resp(true),
    //             srce_ad: reg_bus_addr(0x2060_000c),
    //             dest_ad: mem_bus_addr(test_buf.as_mut_ptr() as usize as u32),
    //             tfr_len: 0x1000,
    //             stride: 0,
    //             next_cb: 0,
    //             debug: 0,
    //             _unused: 0,
    //         },
    //     ];
    //
    //     smi.l.write(2);
    //
    //     smi.dc.modify(|r| r.with_dmaen(true));
    //     smi.cs.modify(|r| r.with_write(true));
    //     smi.cs.modify(|r| r.with_enable(true));
    //     smi.cs.modify(|r| r.with_clear(true));
    //
    //     __dsb();
    //     // enable
    //     (0x2000_7ff0usize as *mut u32).write_volatile(1 << 5);
    //     dma.devices[5].cs.write(DMA_CS(0).with_reset(true));
    //
    //     dma.devices[5]
    //         .conblk_ad
    //         .write(mem_bus_addr(cbs.as_slice().as_ptr() as usize as u32));
    //     dma.devices[5].cs.write(DMA_CS(2));
    //     dma.devices[5].debug.write(7);
    //     dma.devices[5].cs.write(DMA_CS(1));
    //
    //     __dsb();
    //
    //     // force clock sync and give the receiver long enough get ready to read
    //     smi_write(&smi, 0xffff);
    //     // smi_write(&smi, 0x6666);
    //     // smi_wait(&smi);
    //     // delay_micros(st, 35);
    //
    //     __dsb();
    //
    //     smi.cs.modify(|r| r.with_start(true));
    //
    //     __dsb();
    //
    //     while dma.devices[5].txfr_len.read() > 0 {}
    //     while dma.devices[5].cs.read().active() {}
    //
    //     let c1 = cycle_read();
    //
    //     __dsb();
    //
    //     // <READ
    //     while !smi.cs.read().done() {}
    //
    //     // smi_wait(&smi);
    //     // smi.cs.modify(|r| r.with_enable(false));
    //     // smi.cs.modify(|r| r.with_clear(true).with_write(false));
    //     // __dsb();
    //     // </READ
    //     smi.l.write(0x0800);
    //     // <READ
    //     // smi.cs.modify(|r| r.with_pxldat(true));
    //     // smi.cs.modify(|r| r.with_enable(true));
    //     // smi.cs.modify(|r| r.with_clear(true));
    //     // __dsb();
    //     // </READ
    //
    //     // smi.cs.modify(|r| r.with_clear(true));
    //     // __dsb();
    //     // smi.dcs.modify(|r| r.with_enable(false));
    //     // smi.cs.modify(|r| r.with_enable(false));
    //     // __dsb();
    //     // smi.cs.modify(|r| {
    //     //     r.with_write(false).with_clear(true)
    //     //     // .with_seterr(true)
    //     //     // .with_pxldat(true)
    //     // });
    //     // // // uart1_sendln_bl!("SMI_CS={:?}", smi.cs.read());
    //     // __dsb();
    //     // smi.cs.modify(|r| r.with_enable(true));
    //
    //     __dsb();
    //
    //     dma.devices[5].conblk_ad.write(mem_bus_addr(
    //         // cbs.as_slice().as_ptr().offset(2) as usize as u32,
    //         cbs.as_slice().as_ptr().offset(1) as usize as u32,
    //     ));
    //     dma.devices[5].cs.write(DMA_CS(2));
    //     dma.devices[5].debug.write(7);
    //
    //     __dsb();
    //
    //     while peri.GPIO.gplev0().read().lev26().bit_is_set() {}
    //     let c2 = cycle_read();
    //
    //     __dsb();
    //
    //     dma.devices[5].cs.write(DMA_CS(1));
    //
    //     __dsb();
    //
    //     // WRITE:
    //     // smi_write(&smi, 0xffff);
    //     // smi_wait(&smi);
    //     // __dsb();
    //     // delay_micros(st, 0);
    //
    //     __dsb();
    //
    //     smi.cs.modify(|r| r.with_start(true));
    //
    //     __dsb();
    //
    //     while dma.devices[5].txfr_len.read() > 0 {}
    //     while dma.devices[5].cs.read().active() {}
    //
    //     __dsb();
    //
    //     uart1_sendln_bl!("=== transfer finished ({}) ===", c2 - c1);
    //     for i in 0..8 {
    //         let word = test_buf.as_mut_ptr().offset(i).read_volatile();
    //         uart1_sendln_bl!("word at test_buf+{i} is {word:08x}");
    //     }
    // }
}

fn init_programmed_write(n: usize, smi: &SMI) {
    __dsb();
    unsafe {
        smi.cs.modify(|r| r.with_enable(false));
        while smi.cs.read().enable() {}
        smi.l.write(n as u32);
        smi.cs.modify(|r| r.with_write(true).with_enable(true));
        smi.cs.modify(|r| r.with_start(true));
    }
    __dsb();
}

fn write_fifo(buf: &[u32], smi: &SMI) {
    if !smi.cs.read().txe() {
        uart1_sendln_bl!("WARNING: write fifo not empty at start of write call.");
        unsafe {
            smi.cs.modify(|r| r.with_clear(true));
        }
    }
    assert_eq!(buf.len() % 4, 0);
    init_programmed_write(buf.len() * 2, smi);
    for w in buf {
        while !smi.cs.read().txd() {}
        unsafe { smi.d.write(*w) }
    }
    while !smi.cs.read().done() {}
    if !smi.cs.read().txe() {
        uart1_sendln_bl!("WARNING: FIFO not empty at end of write operation.");
    }
}

fn smi_wait(smi: &SMI) {
    unsafe {
        __dsb();
        while !smi.dcs.read().done() {}
        __dsb();
    }
}

pub fn smi_write(smi: &SMI, val: u16) {
    unsafe {
        __dsb();
        smi.dcs.modify(|r| r.with_done(true).with_write(true));
        smi.dd.write(val as u32);
        smi.dcs.modify(|r| r.with_start(true));
        __dsb();
    }
}

fn write_bytes(s: &[u8]) {
    let peri = unsafe { Peripherals::steal() };
    let write_nybble = |n: u8| {
        let set_mask = n;
        let clr_mask = !n;

        let set_mask = (set_mask as u32) << 18;
        let clr_mask = (clr_mask as u32) << 18;

        __dsb();
        unsafe {
            peri.GPIO.gpclr0().write_with_zero(|w| w.bits(clr_mask));
            peri.GPIO
                .gpset0()
                .write_with_zero(|w| w.bits(set_mask).set27().set_bit());
        }
        __dsb();
        unsafe {
            peri.GPIO
                .gpclr0()
                .write_with_zero(|w| w.clr24().clear_bit_by_one());
        }
        __dsb();
        delay_micros(&peri.SYSTMR, 10);
        __dsb();
        unsafe {
            peri.GPIO.gpset0().write_with_zero(|w| w.set24().set_bit());
            peri.GPIO
                .gpclr0()
                .write_with_zero(|w| w.clr27().clear_bit_by_one());
        }
        __dsb();
        delay_micros(&peri.SYSTMR, 10);
        __dsb();
    };
    for &b in s {
        let first = b >> 4;
        let second = b & 0xf;
        write_nybble(first);
        write_nybble(second);
    }
}

#[no_mangle]
pub extern "C" fn __aeabi_unwind_cpp_pr0() {}
