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
#![feature(vec_into_raw_parts)]
#![no_std]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use bcm2835_lpa::Peripherals;
use bismuth::arch::arm1176::__dsb;
use bismuth::arch::arm1176::pmm::RegionKind;
use bismuth::arch::arm1176::timing::{cycle_init, cycle_read, delay_micros};
use bismuth::boot::PMM;
use bismuth::kalloc::SimpleAlloc;
use bismuth::peripherals::dma::{DMA, DMA_CB, DMA_CS, DMA_TI};
use bismuth::peripherals::smi::{
    smi_init, SMIConfig, SMIDataWidth, CM_SMI, SMI, SMI_A, SMI_CS, SMI_DA, SMI_DCS,
};
use bismuth::sync::once::OnceLock;
use bismuth::{uart1_sendln_bl, KiB, MiB};
use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;
use embedded_alloc::Heap;

#[global_allocator]
static HEAP: Heap = Heap::empty();

#[no_mangle]
pub extern "C" fn __bis__main() {
    uart1_sendln_bl!("=== RPI-DOWNLOADMOARRAM SERVER ===");
    {
        const HEAP_SIZE: usize = 16 * MiB;
        let heap_mem = (&mut PMM.get().lock())
            .allocate_region(RegionKind::Supersection)
            .unwrap();
        uart1_sendln_bl!("HEAP_MEM: {heap_mem:p}-{:p}", unsafe {
            heap_mem.byte_offset(HEAP_SIZE as isize)
        },);
        unsafe {
            HEAP.init(heap_mem as usize, HEAP_SIZE);
        }
    }
    // GLOBAL_ALLOC.0.get_or_init(|| {
    //     SimpleAlloc::new(
    //         (&mut PMM.get().lock())
    //             .allocate_region(RegionKind::Supersection)
    //             .unwrap(),
    //         16 * MiB,
    //     )
    // });
    let peri = unsafe { Peripherals::steal() };

    let st = &peri.SYSTMR;

    __dsb();
    unsafe {
        peri.GPIO.gpfsel0().modify(|_, w| {
            w
                // SERVER: listen for pattern 01
                .fsel0()
                .input()
                .fsel1()
                .input()
                // /END SERVER
                // SERVER: address
                .fsel2()
                .input()
                .fsel3()
                .input()
                .fsel4()
                .input()
                .fsel5()
                .input()
                // /END SERVER
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
                // .input()
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
                // .fsel24()
                // .sd16()
                // .fsel25()
                // .sd17()
                .fsel26()
                .output()
        });
        peri.GPIO.gpfen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gpren0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gpafen0().write_with_zero(|w| w.bits(0x0300_00ff));
        peri.GPIO.gparen0().write_with_zero(|w| w.bits(0x0300_0000));
        peri.GPIO.gplen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gphen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gpset0().write_with_zero(|w| w.set26().set_bit());
    }
    __dsb();

    uart1_sendln_bl!("selected GPIO ALTs");

    cycle_init();

    let cm_smi = unsafe { CM_SMI::steal() };
    let smi = unsafe { SMI::steal() };
    let dma = unsafe { DMA::steal() };

    smi_init(
        st,
        &cm_smi,
        &smi,
        SMIConfig {
            width: SMIDataWidth::Bits16,
            clock_ns: 1000,
            // we warp this a LOT so that we can still pick up
            setup_cycles: 5,
            strobe_cycles: 5,
            hold_cycles: 60,
            pace_cycles: 0,
        },
    );

    fn mem_bus_addr(p: u32) -> u32 {
        // FUCK YOU BROADCOM
        // I WASTED 5 HOURS BECAUSE YOU GAVE ME THE WRONG GODDAMN VALUE
        0x4000_0000 + p
    }
    fn reg_bus_addr(p: u32) -> u32 {
        0x7e00_0000 + (p - 0x2000_0000)
    }

    unsafe {
        let src_p = 0x1000_0000usize as *mut u32;
        let dst_p = 0x1000_1000usize as *mut u32;
        for i in 0..64 {
            src_p.offset(i).write_volatile(i as u32);
            dst_p.offset(i).write_volatile(0u32);
        }
        let test_read_buf = 0x1000_2000usize as *mut u32;
        for i in 0..1024 {
            test_read_buf.offset(1023 - i).write(i as u32);
        }
        let mut cbs_p = 0x1000_3000usize as *mut DMA_CB;
        for i in 0..16 {
            cbs_p.offset(i).write(DMA_CB {
                ti: DMA_TI(0),
                srce_ad: 0,
                dest_ad: 0,
                tfr_len: 0,
                stride: 0,
                next_cb: 0,
                debug: 0,
                _unused: 0,
            });
        }

        __dsb();
        // EDIT
        smi.devices[0].dsr.modify(|r| r.with_rpaceall(true));
        // smi.devices[0].dsw.modify(|r| r.with_wpaceall(true));
        // smi.dc.modify(|r| r.with_dmap(true));
        // smi.devices[0].dsr.modify(|r| r.with_rdreq(true));
        // END EDIT
        smi.dc
            .modify(|r| r.with_reqr(2).with_reqw(2).with_panicr(8).with_panicw(8));
        smi.dc.modify(|r| r.with_dmaen(true));
        smi.cs.modify(|r| r.with_enable(true));
        smi.cs.modify(|r| r.with_clear(true));
        __dsb();

        let mut cmd_buf: [u32; 1] = [0; 1];
        // rxbuff = adc_dma_start(&vc_mem, nsamples)
        (0x2000_7ff0usize as *mut u32).write_volatile((1 << 5));
        dma.devices[5].cs.write(DMA_CS(0).with_reset(true));
        cbs_p.offset(0).write(DMA_CB {
            ti: DMA_TI(0)
                .with_src_dreq(true)
                .with_dest_inc(true)
                .with_permap(4)
                .with_wait_resp(true),
            srce_ad: reg_bus_addr(0x2060_000c),
            dest_ad: mem_bus_addr(cmd_buf.as_mut_ptr() as usize as u32),
            tfr_len: 4,
            stride: 0,
            next_cb: 0,
            debug: 0,
            _unused: 0,
        });
        // cbs[0].dest_ad = mem_bus_addr(cbs.as_slice().as_ptr().offset(1) as usize as u32);
        // cbs[0].next_cb = mem_bus_addr(cbs.as_slice().as_ptr().offset(1) as usize as u32);
        dma.devices[5]
            .conblk_ad
            .write(mem_bus_addr(cbs_p as usize as u32));
        dma.devices[5].cs.write(DMA_CS(2));
        dma.devices[5].debug.write(7);
        dma.devices[5].cs.write(DMA_CS(1));

        // dma.devices[7]
        //     .conblk_ad
        //     .write(mem_bus_addr(cbs_p.offset(1) as usize as u32));

        // cbs[0].dest_ad = mem_bus_addr(cbs.as_ptr().offset(1) as usize as u32 - 4);
        // smi_start(NSAMPLES, 1)
        __dsb();
        // smi.l.write(0x110);
        smi.l.write(2);
        smi.cs.modify(|r| r.with_pxldat(true));
        smi.cs.modify(|r| r.with_enable(true));
        smi.cs.modify(|r| r.with_clear(true));
        __dsb();

        // smi_cs_cached.write_volatile(smi.cs.read().with_start(true).0);

        __dsb();

        loop {
            unsafe {
                let eds = peri.GPIO.gpeds0().read();
                if eds.bits() != 0 {
                    let lev = peri.GPIO.gplev0().read().bits();
                    __dsb();
                    // server select
                    // SA5 high, SA4 low, SA3:SA0 = 0000
                    if lev & 0x3f == 0x20 {
                        smi.cs.modify(|r| r.with_start(true));
                        __dsb();
                        while dma.devices[5].cs.read().active() {}
                        __dsb();
                        let c1 = cycle_read();
                        let command = cmd_buf[0];
                        let size_specifier = command & 3;
                        let byte_count = match size_specifier {
                            0 => 4 * KiB,
                            1 => 64 * KiB,
                            2 => 1 * MiB,
                            3 => 16 * MiB,
                            _ => unreachable!(),
                        };
                        let addr = command & 0x3fff_fffc;
                        if (command & 0x8000_0000) == 0x8000_0000 {
                            // write
                            cbs_p.offset(1).write(DMA_CB {
                                ti: DMA_TI(0)
                                    .with_src_dreq(true)
                                    .with_dest_inc(true)
                                    .with_permap(4)
                                    .with_wait_resp(true),
                                srce_ad: reg_bus_addr(0x2060_000c),
                                dest_ad: mem_bus_addr(addr),
                                tfr_len: byte_count as u32,
                                stride: 0,
                                next_cb: 0,
                                debug: 0,
                                _unused: 0,
                            });
                            __dsb();
                            smi.l.write(byte_count as u32 / 2);
                            __dsb();
                            dma.devices[5]
                                .conblk_ad
                                .write(mem_bus_addr(cbs_p.offset(1) as usize as u32));
                            dma.devices[5].cs.write(DMA_CS(2));
                            dma.devices[5].debug.write(7);
                            __dsb();
                            peri.GPIO
                                .gpclr0()
                                .write_with_zero(|w| w.clr26().clear_bit_by_one());
                            // timing issues...
                            asm!("nop", "nop", "nop", "nop");
                            asm!("nop", "nop", "nop", "nop");
                            asm!("nop", "nop", "nop", "nop");
                            asm!("nop", "nop", "nop", "nop");

                            asm!("nop", "nop", "nop", "nop");
                            asm!("nop", "nop", "nop", "nop");
                            asm!("nop", "nop", "nop", "nop");
                            asm!("nop", "nop", "nop", "nop");

                            asm!("nop", "nop", "nop", "nop");
                            asm!("nop", "nop", "nop", "nop");
                            asm!("nop", "nop", "nop", "nop");
                            asm!("nop", "nop", "nop", "nop");
                            // __dsb();
                            // smi.cs.modify(|r| r.with_pad(2));
                            __dsb();
                            dma.devices[5].cs.write(DMA_CS(1));
                            __dsb();
                            let c2 = cycle_read();
                            smi.cs.modify(|r| r.with_start(true));
                            __dsb();
                            peri.GPIO.gpset0().write_with_zero(|w| w.set26().set_bit());
                            __dsb();
                            while dma.devices[5].cs.read().active() {}
                            while dma.devices[5].txfr_len.read() > 0 {}
                            __dsb();
                            uart1_sendln_bl!("=== transfer finished ({}) ===", c2 - c1);
                            for i in 0..8 {
                                let word_ptr = (addr as usize as *mut u32).offset(i);
                                let word = word_ptr.read_volatile();
                                uart1_sendln_bl!("word at {word_ptr:p} is {word:08x}");
                            }
                        } else {
                            // read
                            cbs_p.offset(1).write(DMA_CB {
                                ti: DMA_TI(0)
                                    .with_dest_dreq(true)
                                    .with_permap(4)
                                    .with_src_inc(true),
                                srce_ad: mem_bus_addr(addr),
                                dest_ad: reg_bus_addr(0x2060_000c),
                                tfr_len: byte_count as u32,
                                stride: 0,
                                next_cb: 0,
                                debug: 0,
                                _unused: 0,
                            });
                            __dsb();
                            smi.l.write(byte_count as u32 / 2);
                            smi.cs.modify(|r| r.with_write(true));
                            __dsb();
                            dma.devices[5]
                                .conblk_ad
                                .write(mem_bus_addr(cbs_p.offset(1) as usize as u32));
                            dma.devices[5].cs.write(DMA_CS(2));
                            dma.devices[5].debug.write(7);
                            __dsb();
                            peri.GPIO
                                .gpclr0()
                                .write_with_zero(|w| w.clr26().clear_bit_by_one());
                            // timing issues...
                            // asm!("nop", "nop", "nop", "nop");
                            // asm!("nop", "nop", "nop", "nop");
                            // asm!("nop", "nop", "nop", "nop");
                            // asm!("nop", "nop", "nop", "nop");
                            //
                            // asm!("nop", "nop", "nop", "nop");
                            // asm!("nop", "nop", "nop", "nop");
                            // asm!("nop", "nop", "nop", "nop");
                            // asm!("nop", "nop", "nop", "nop");
                            //
                            // asm!("nop", "nop", "nop", "nop");
                            // asm!("nop", "nop", "nop", "nop");
                            // asm!("nop", "nop", "nop", "nop");
                            // asm!("nop", "nop", "nop", "nop");
                            // __dsb();
                            // smi.cs.modify(|r| r.with_pad(2));
                            __dsb();
                            dma.devices[5].cs.write(DMA_CS(1));
                            __dsb();
                            let c2 = cycle_read();
                            smi.cs.modify(|r| r.with_start(true));
                            __dsb();
                            peri.GPIO.gpset0().write_with_zero(|w| w.set26().set_bit());
                            __dsb();
                            while dma.devices[5].cs.read().active() {}
                            while dma.devices[5].txfr_len.read() > 0 {}
                            __dsb();
                            uart1_sendln_bl!(
                                "=== transfer finished ({}cy): {addr:08x}: {byte_count:08x} ===",
                                c2 - c1
                            );
                            for i in 0..8 {
                                let word_ptr = (addr as usize as *mut u32).offset(i);
                                let word = word_ptr.read_volatile();
                                uart1_sendln_bl!("word at {word_ptr:p} is {word:08x}");
                            }
                            smi.cs.modify(|r| r.with_write(false));
                        }
                        // uart1_sendln_bl!("==== BUFFER ====");
                        // for word in buf {
                        //     uart1_sendln_bl!("{word:08x}");
                        // }
                    }
                    unsafe { peri.GPIO.gpeds0().write_with_zero(|w| w.bits(0xffff_ffff)) };
                }
            }
        }
    }

    // unsafe {
    //     __dsb();
    //     // smi.a.write(SMI_A(0).with_device(0).with_address(0b000011));
    //     // smi.da.write(SMI_DA(0).with_device(0).with_address(0b111111));
    //     smi.cs
    //         .modify(|r| r.with_clear(true).with_aferr(true).with_pxldat(true));
    //     // the wisdom of the ancients
    //     smi.dcs.modify(|r| r.with_enable(true));
    //     __dsb();
    // }

    // // let mut buf = vec![0u16; 0x100];
    // // // let mut i = 0;
    // // let mut buf = vec![0; 0x88];
    // let mut cmd_buf = [0u32; 3];
    // loop {
    //     unsafe {
    //         let c1 = cycle_read();
    //         let eds = peri.GPIO.gpeds0().read();
    //         if eds.bits() != 0 {
    //             let lev = peri.GPIO.gplev0().read().bits();
    //             // server select
    //             // SA5 high, SA4 low, SA3:SA0 = 0000
    //             if lev & 0x3f == 0x20 {
    //                 // let hw = ((lev & 0x00ffff00) >> 8) as u16;
    //                 // buf[i] = hw;
    //                 // i += 1;
    //                 // if i == 0x100 {
    //                 //     // 256 * 8 = 2048B, 115200 ~ 11.52kB/s
    //                 //     for &b in &buf {
    //                 //         uart1_sendln_bl!("{b:08x}");
    //                 //     }
    //                 //     // buf.clear();
    //                 //     i = 0;
    //                 // }
    //                 read_fifo(&mut cmd_buf, &smi);
    //
    //                 uart1_sendln_bl!("received:");
    //                 for &b in &cmd_buf {
    //                     uart1_sendln_bl!("{b:08x}");
    //                 }
    //                 uart1_sendln_bl!("c1={c1}");
    //                 uart1_sendln_bl!("lev={lev:08x}");
    //             }
    //             unsafe { peri.GPIO.gpeds0().write_with_zero(|w| w.bits(0xffff_ffff)) };
    //         }
    //     }
    // }

    fn smi_read_single_word(smi: &SMI) -> u32 {
        unsafe {
            smi.dcs.write(SMI_DCS(0).with_enable(true));
            smi.dcs.write(SMI_DCS(0).with_enable(true).with_start(true));
            __dsb();
            while !smi.dcs.read().done() {}
            let r = smi.dd.read();
            __dsb();
            r
        }
    }

    fn read_fifo(buf: &mut [u32], smi: &SMI) {
        if smi.cs.read().rxd() {
            uart1_sendln_bl!("WARNING: SMI RX FIFO populated at start of read");
            while smi.cs.read().rxd() {}
        }

        // let c2 = cycle_read();

        // hehe
        smi_init_programmed_read(buf.len() / 2, smi);

        // let c3 = cycle_read();
        let mut i = 0;
        loop {
            let cs = smi.cs.read();
            if cs.rxd() {
                let w = smi.d.read();
                // uart1_sendln_bl!("{w:08x}");
                buf[i] = w;
                i += 1;
            }
            if cs.done() {
                break;
            }
        }
        if smi.cs.read().rxd() {
            let fifo_count = smi.fd.read().fcnt() as usize;
            for _ in 0..fifo_count {
                let w = smi.d.read();
                // uart1_sendln_bl!("{w:08x}");
                buf[i] = w;
                i += 1;
            }
        }
        if !smi.cs.read().done() {
            uart1_sendln_bl!("WARNING: transaction finished but DONE bit not set.");
        }
        if smi.cs.read().rxd() {
            uart1_sendln_bl!("WARNING: read FIFO not empty at end of read call.");
        }
        // uart1_sendln_bl!("c2={c2}");
        // uart1_sendln_bl!("c3={c3}");
    }

    fn smi_init_programmed_read(n_bytes: usize, smi: &SMI) {
        unsafe {
            smi.cs.modify(|r| r.with_enable(false).with_write(false));
            while smi.cs.read().enable() {}

            smi.l.write(n_bytes as u32);

            smi.cs.modify(|r| r.with_enable(true));
            __dsb();
            while smi.cs.read().active() {}
            smi.cs.modify(|r| r.with_clear(true));
            smi.cs.modify(|r| r.with_start(true));
        }
    }

    // unsafe { peri.GPIO.gpfen0().write_with_zero(|w| w.bits(0x0000_0000)) }
    // unsafe { peri.GPIO.gpren0().write_with_zero(|w| w.bits(0x0000_0000)) }
    // unsafe { peri.GPIO.gpafen0().write_with_zero(|w| w.bits(0x0000_00c0)) }
    // unsafe { peri.GPIO.gparen0().write_with_zero(|w| w.bits(0x0000_00c0)) }
    // unsafe { peri.GPIO.gplen0().write_with_zero(|w| w.bits(0x0000_0000)) }
    // unsafe { peri.GPIO.gphen0().write_with_zero(|w| w.bits(0x0000_0000)) }
    //
    // __dsb();
    //
    // let mut arr = [0; 24];
    //
    // loop {
    //     __dsb();
    //     while (peri.GPIO.gpeds0().read().bits() & 0b11000000) == 0 {}
    //     unsafe { peri.GPIO.gpeds0().write_with_zero(|w| w.bits(0xffffffff)) }
    //     let lev = peri.GPIO.gplev0().read().bits();
    //     __dsb();
    //     for i in 0..24 {
    //         arr[i] = if lev & (0x80_0000 >> i) == 0 {
    //             b'0'
    //         } else {
    //             b'1'
    //         };
    //     }
    //
    //     uart1_sendln_bl!("{}", unsafe { core::str::from_utf8_unchecked(&arr) });
    // }
}
//
// /* */
// pub struct SimpleGlobal(pub(crate) OnceLock<SimpleAlloc>);
//
// unsafe impl GlobalAlloc for SimpleGlobal {
//     unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
//         self.0.get().unwrap().alloc(layout)
//     }
//
//     unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
//         self.0.get().unwrap().dealloc(ptr, layout)
//     }
// }
//
#[no_mangle]
pub extern "C" fn __aeabi_unwind_cpp_pr0() {}

// #[global_allocator]
// pub(crate) static GLOBAL_ALLOC: SimpleGlobal = SimpleGlobal(OnceLock::new());
// /* */
