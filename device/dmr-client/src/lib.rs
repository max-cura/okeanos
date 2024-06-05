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

    // let mut recv_buf = [0u32; 1024];
    // let c1 = cycle_read();
    // // read from remote into recv_buf
    // request::dmr_issue_read_command(
    //     0x10002000,
    //     RegionKind::SmallPage,
    //     recv_buf.as_mut_ptr(),
    //     &smi,
    //     &dma,
    //     &peri.GPIO,
    //     0b000001,
    // );
    // let c2 = cycle_read();
    // uart1_sendln_bl!("=== transfer (1) finished ({}) ===", c2 - c1);
    // for i in 0..8 {
    //     let word = unsafe { recv_buf.as_mut_ptr().offset(i).read_volatile() };
    //     uart1_sendln_bl!("word at recv_buf+{i} is {word:08x}");
    // }

    // uart1_sendln_bl!("starting second transaction...");
    //
    // let mut recv_buf = [0u32; 1024];
    // let c1 = cycle_read();
    // request::dmr_issue_read_command(
    //     0x10002000,
    //     RegionKind::SmallPage,
    //     recv_buf.as_mut_ptr(),
    //     &smi,
    //     &dma,
    //     &peri.GPIO,
    //     0b000001,
    // );
    // let c2 = cycle_read();
    // uart1_sendln_bl!("=== transfer (1) finished ({}) ===", c2 - c1);
    // for i in 0..8 {
    //     let word = unsafe { recv_buf.as_mut_ptr().offset(i).read_volatile() };
    //     uart1_sendln_bl!("word at recv_buf+{i} is {word:08x}");
    // }

    // let mut send_buf = [0u32; 1024];
    // for i in 0..1024 {
    //     send_buf[i] = i as u32 * 0x0001_0001;
    // }
    // let c1 = cycle_read();
    // // write into remote from send_buf
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
}

pub fn passthru_write_bytes_blocking(bytes: &[u8]) {
    let smi = unsafe { SMI::steal() };
    let st = unsafe { SYSTMR::steal() };
    let da = smi.da.read();
    unsafe {
        smi.da.write(SMI_DA(0));
    }
    for &b in bytes {
        smi_write(&smi, (b as u16) << 8);
        delay_micros(&st, 10);
    }
    unsafe {
        smi.da.write(da);
    }
}

// fn init_programmed_write(n: usize, smi: &SMI) {
//     __dsb();
//     unsafe {
//         smi.cs.modify(|r| r.with_enable(false));
//         while smi.cs.read().enable() {}
//         smi.l.write(n as u32);
//         smi.cs.modify(|r| r.with_write(true).with_enable(true));
//         smi.cs.modify(|r| r.with_start(true));
//     }
//     __dsb();
// }
//
// fn write_fifo(buf: &[u32], smi: &SMI) {
//     if !smi.cs.read().txe() {
//         uart1_sendln_bl!("WARNING: write fifo not empty at start of write call.");
//         unsafe {
//             smi.cs.modify(|r| r.with_clear(true));
//         }
//     }
//     assert_eq!(buf.len() % 4, 0);
//     init_programmed_write(buf.len() * 2, smi);
//     for w in buf {
//         while !smi.cs.read().txd() {}
//         unsafe { smi.d.write(*w) }
//     }
//     while !smi.cs.read().done() {}
//     if !smi.cs.read().txe() {
//         uart1_sendln_bl!("WARNING: FIFO not empty at end of write operation.");
//     }
// }
//
// fn smi_wait(smi: &SMI) {
//     unsafe {
//         __dsb();
//         while !smi.dcs.read().done() {}
//         __dsb();
//     }
// }

pub fn smi_write(smi: &SMI, val: u16) {
    unsafe {
        __dsb();
        smi.dcs.modify(|r| r.with_done(true).with_write(true));
        smi.dd.write(val as u32);
        smi.dcs.modify(|r| r.with_start(true));
        __dsb();
    }
}

// fn write_bytes(s: &[u8]) {
//     let peri = unsafe { Peripherals::steal() };
//     let write_nybble = |n: u8| {
//         let set_mask = n;
//         let clr_mask = !n;
//
//         let set_mask = (set_mask as u32) << 18;
//         let clr_mask = (clr_mask as u32) << 18;
//
//         __dsb();
//         unsafe {
//             peri.GPIO.gpclr0().write_with_zero(|w| w.bits(clr_mask));
//             peri.GPIO
//                 .gpset0()
//                 .write_with_zero(|w| w.bits(set_mask).set27().set_bit());
//         }
//         __dsb();
//         unsafe {
//             peri.GPIO
//                 .gpclr0()
//                 .write_with_zero(|w| w.clr24().clear_bit_by_one());
//         }
//         __dsb();
//         delay_micros(&peri.SYSTMR, 10);
//         __dsb();
//         unsafe {
//             peri.GPIO.gpset0().write_with_zero(|w| w.set24().set_bit());
//             peri.GPIO
//                 .gpclr0()
//                 .write_with_zero(|w| w.clr27().clear_bit_by_one());
//         }
//         __dsb();
//         delay_micros(&peri.SYSTMR, 10);
//         __dsb();
//     };
//     for &b in s {
//         let first = b >> 4;
//         let second = b & 0xf;
//         write_nybble(first);
//         write_nybble(second);
//     }
// }

#[no_mangle]
pub extern "C" fn __aeabi_unwind_cpp_pr0() {}
