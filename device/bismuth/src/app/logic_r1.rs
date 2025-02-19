use crate::app::dma::{DMA, DMA_CB, DMA_CS, DMA_TI};
use crate::app::smi::{CM_SMI, SMI, SMI_DA, SMIConfig, SMIDataWidth, smi_init};
use crate::exceptions::VectorBuilder;
use crate::int::{InterruptMode, OperatingMode};
use crate::{def_irq_landing_pad, int, steal_println};
use alloc::boxed::Box;
use alloc::vec;
use bcm2835_lpa::{GPIO, Peripherals};
use core::alloc::Layout;
use core::arch::global_asm;
use core::sync::atomic::{AtomicBool, Ordering};
use quartz::arch::arm1176::{__dsb, cycle};
use quartz::device::bcm2835::timing::delay_millis;
use quartz::sync::once::OnceLock;
use volatile_register::RW;
// unsafe extern "C" {
//     static FIQ_HANDLER_SYM: *mut [u32; 0];
// }
//
// global_asm!(
//     ".globl {FIQ_HANDLER}",
//     ".extern {FIQ_PASSTHRU}",
//     "{FIQ_HANDLER}:",
//     "   srsdb #{FIQ_MODE}",
//     "   push {{r0-r12}}",
//     "   bl {FIQ_PASSTHRU}",
//     "   pop {{r0-r12}}",
//     "   rfedb sp",
//     FIQ_HANDLER = sym FIQ_HANDLER_SYM,
//     FIQ_MODE = const 0b10001,
//     FIQ_PASSTHRU = sym fiq,
// );
//
// extern "C" fn fiq() {
//     let peri = unsafe { Peripherals::steal() };
//     __dsb();
//     let masked_bits = peri.GPIO.gpeds0().read().bits() & 0x0fff_3fff;
//     unsafe { peri.GPIO.gpeds0().write_with_zero(|w| w) };
//     if masked_bits == 0 {
//         __dsb();
//         return;
//     } else {
//         start_adc_read();
//         __dsb();
//     }
// }

def_irq_landing_pad!(local_irq2, handle_irq2);

fn handle_irq2() {
    let peripherals = unsafe { Peripherals::steal() };
    let pending2 = peripherals.LIC.pending_2().read();
    if !pending2.gpio_0().bit_is_set() {
        return;
    }
    let mask = peripherals.GPIO.gpeds0().read().bits();
    let _bits = peripherals.GPIO.gplev0().read().bits();
    // steal_println!("bits={bits:b} mask={mask:b}");
    const WORDS: usize = 16;
    let mut mem = vec![0u32; WORDS];
    let _cm_smi = unsafe { CM_SMI::steal() };
    let smi = unsafe { SMI::steal() };
    let dma = unsafe { DMA::steal() };
    unsafe {
        __dsb();
        (&*DMA_ENABLE_REG).write(1 << 5);
        dma.devices[5].cs.write(DMA_CS(0).with_reset(true));
    }
    unsafe {
        let cbs = vec![DMA_CB {
            ti: DMA_TI(0)
                .with_src_dreq(true)
                .with_dest_inc(true)
                .with_permap(4)
                .with_wait_resp(true),
            srce_ad: reg_bus_addr(0x2060_000c),
            dest_ad: mem_bus_addr(mem.as_mut_ptr().expose_provenance() as u32),
            tfr_len: (WORDS * 4) as u32, // bytes
            stride: 0,
            next_cb: 0,
            debug: 0,
            _unused: 0,
        }];
        dma.devices[5]
            .conblk_ad
            .write(mem_bus_addr(cbs.as_ptr().expose_provenance() as u32));
        dma.devices[5].cs.write(DMA_CS(2));
        dma.devices[5].debug.write(7);
        dma.devices[5].cs.write(DMA_CS(1));

        __dsb();
        smi.l.write((WORDS * 2) as u32);
        smi.cs.modify(|r| r.with_pxldat(true));
        smi.cs.modify(|r| r.with_enable(true));
        smi.cs.modify(|r| r.with_clear(true));
        smi.cs.modify(|r| r.with_start(true));
        __dsb();
        while dma.devices[5].cs.read().active() {}
        __dsb();

        steal_println!(
            "irq: {:04x?}",
            bytemuck::cast_slice::<u32, u16>(mem.as_slice())
        );
        __dsb();
        unsafe { peripherals.GPIO.gpeds0().write_with_zero(|w| w.bits(mask)) };
        __dsb();
    }
}

static ADC_BUF: OnceLock<Box<[u16]>> = OnceLock::new();
static RUNNING: AtomicBool = AtomicBool::new(false);

fn start_adc_read() {
    if RUNNING.load(Ordering::SeqCst) {
        return;
    }
    RUNNING.store(true, Ordering::SeqCst);
}

/// RAM
const fn mem_bus_addr(p: u32) -> u32 {
    0x4000_0000 + p
}
/// MMIO
const fn reg_bus_addr(p: u32) -> u32 {
    0x7e00_0000 + (p - 0x2000_0000)
}

const SMI_D_REG_PHY: u32 = 0x2060_000cu32;
const DMA_ENABLE_REG: *mut RW<u32> = 0x2000_7ff0usize as *mut RW<u32>;

const DMA_PERMAP_SMI: u8 = 4;

pub fn run() {
    let peri = unsafe { Peripherals::steal() };
    steal_println!("allocating vector destination");

    // allocate space for vector table
    let layout = Layout::array::<u32>(8).unwrap();
    let vdi_ptr: *mut u32 = core::ptr::with_exposed_provenance_mut(0);
    let vdi_len = layout.size();
    let vector_dst = core::ptr::slice_from_raw_parts_mut(vdi_ptr, vdi_len);

    steal_println!("clearing LIC IRQs");

    __dsb();
    peri.LIC
        .disable_1()
        .write(|w| unsafe { w.bits(0xffff_ffff) });
    peri.LIC
        .disable_2()
        .write(|w| unsafe { w.bits(0xffff_ffff) });
    peri.LIC
        .disable_basic()
        .write(|w| unsafe { w.bits(0xffff_ffff) });
    __dsb();

    steal_println!("initializing GPIO FSELs");
    unsafe {
        peri.GPIO.gpfsel0().modify(|_, w| {
            // we use the strobe for clocking
            w.fsel6().soe_n().fsel7().input()
        });
        peri.GPIO.gpfsel1().modify(|_, w| {
            w.fsel10()
                .sd2()
                .fsel12()
                .pwm0_0()
                .fsel13()
                .sd5()
                // don't touch 14 and 15
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
        });
        peri.GPIO.gpfen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gpren0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO
            .gpafen0()
            .write_with_zero(|w| w.bits(0x0000_0000).afen10().set_bit());
        peri.GPIO
            .gparen0()
            .write_with_zero(|w| w.bits(0x0000_0000).aren10().set_bit());
        peri.GPIO.gplen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gphen0().write_with_zero(|w| w.bits(0x0000_0000));
        peri.GPIO.gpeds0().write_with_zero(|w| w.bits(0xffff_ffff));
    }
    steal_println!("enabling LIC bank 2");
    __dsb();
    peri.LIC.enable_2().write(|w| w.gpio_0().set_bit());
    __dsb();

    steal_println!("installing vector table");
    // fill in vector table
    let vector = VectorBuilder::new().set_irq_handler(unsafe { &raw const local_irq2 });
    unsafe { vector.install(vector_dst) }.unwrap();

    steal_println!("initializing IRQ stack");
    unsafe { int::init_stack_for_mode(OperatingMode::IRQ, 0x0800_0000) };

    // pwm init
    steal_println!("initializing PWM0 on channel 2");
    // XOSC=19.2MHz, divi=32, result=600kHz
    // 600kHz / ran=16 = 37500Hz
    // 26.[6] us
    // we see 10 high 10 low
    // so every ~1.3us-ish?
    // okay, need wider
    // divi=1024 =
    __dsb();
    peri.CM_PWM.cs().write(|w| w.passwd().passwd().src().xosc());
    __dsb();
    delay_millis(&peri.SYSTMR, 110);
    __dsb();
    while peri.CM_PWM.cs().read().busy().bit_is_set() {
        delay_millis(&peri.SYSTMR, 1);
    }
    peri.CM_PWM
        .div()
        .write(|w| w.passwd().passwd().divi().variant(1024));
    peri.CM_PWM
        .cs()
        .write(|w| w.passwd().passwd().enab().set_bit().src().xosc());
    __dsb();
    peri.PWM0
        .ctl()
        .modify(|_, w| w.mode1().pwm().msen1().set_bit().pwen1().set_bit());
    peri.PWM0.rng1().write(|w| unsafe { w.bits(16) });
    peri.PWM0.dat1().write(|w| unsafe { w.bits(8) });
    __dsb();

    // loop {
    //     for i in 0..1024 {
    //         peri.PWM0.dat1().write(|w| unsafe { w.bits(i) });
    //         delay_millis(&peri.SYSTMR, 1);
    //     }
    // }

    steal_println!("initializing counters");

    cycle::enable_counters();
    cycle::ccr_reset();

    steal_println!("initializing SMI");

    let cm_smi = unsafe { CM_SMI::steal() };
    let smi = unsafe { SMI::steal() };

    __dsb();
    smi_init(&peri.SYSTMR, &cm_smi, &smi, SMIConfig {
        width: SMIDataWidth::Bits16,
        clock_ns: 1000, // 1us
        setup_cycles: 10,
        strobe_cycles: 25,
        hold_cycles: 50,
        pace_cycles: 25,
    });
    unsafe {
        smi.devices[0].dsr.modify(|r| r.with_rpaceall(true));
        smi.dc
            .modify(|r| r.with_reqr(2).with_reqw(2).with_panicr(8).with_panicw(8));
        smi.dc.modify(|r| r.with_dmaen(true));
        smi.cs.modify(|r| r.with_enable(true));
        smi.cs.modify(|r| r.with_clear(true));
    }
    __dsb();

    steal_println!("done with init");

    let _ = int::set_enabled_interrupts(InterruptMode::IrqOnly);

    loop {
        __dsb();
        peri.PWM0.ctl().modify(|_, w| w.pwen1().clear_bit());
        __dsb();
        int::set_enabled_interrupts(InterruptMode::Neither);
        steal_println!("---- THING0 ----");
        int::set_enabled_interrupts(InterruptMode::IrqOnly);
        delay_millis(&peri.SYSTMR, 5000);
        __dsb();
        peri.PWM0.ctl().modify(|_, w| w.pwen1().set_bit());
        __dsb();
        int::set_enabled_interrupts(InterruptMode::Neither);
        steal_println!("---- THING1 ----");
        int::set_enabled_interrupts(InterruptMode::IrqOnly);
        delay_millis(&peri.SYSTMR, 5000);
    }
}

// pub fn dma_read_from_smi(
//     ptr: *mut u32,
//     len: usize,
//     smi: &SMI,
//     dma: &DMA,
//     gpio: &GPIO,
//     device_addr: u8,
// ) {
//     unsafe {
//         // Disable SMI so we can change parameters
//         smi.cs.modify(|r| r.with_enable(false));
//         // Clear FIFOs and switch to read mode
//         smi.cs.modify(|r| r.with_clear(true).with_write(true));
//
//         smi.cs.modify(|r| r.with_pxldat(true));
//         smi.cs.modify(|r| r.with_enable(true));
//         smi.cs.modify(|r| r.with_clear(true));
//     }
//     unsafe {
//         smi.a.modify(|r| r.with_address(device_addr));
//         smi.da.modify(|r| r.with_address(device_addr));
//     }
//     let dma_cbs = vec![DMA_CB {
//         ti: DMA_TI(0)
//             .with_dest_dreq(true)
//             .with_dest_inc(true)
//             .with_permap(4 /*DMAP_PERMAP_SMI*/)
//             .with_wait_resp(true),
//         srce_ad: reg_bus_addr(SMI_D_REG_PHY),
//         dest_ad: mem_bus_addr(ptr.expose_provenance() as u32),
//         tfr_len: len as u32,
//         stride: 0, // 2d thing
//         next_cb: 0,
//         debug: 0,
//         _unused: 0,
//     }];
//     __dsb();
//     while !smi.cs.read().done() {}
//
//     unsafe {
//         // Disable SMI so we can change parameters
//         smi.cs.modify(|r| r.with_enable(false));
//         // Clear FIFOs and switch to read mode
//         smi.cs.modify(|r| r.with_clear(true).with_write(false));
//         // Set transaction size to number of u16's that will be read
//         smi.l.write({ len / 2 } as u32);
//
//         smi.cs.modify(|r| r.with_pxldat(true));
//         smi.cs.modify(|r| r.with_enable(true));
//         smi.cs.modify(|r| r.with_clear(true));
//     }
//
//     __dsb();
//
//     let cbs_ptr = dma_cbs.as_slice().as_ptr();
//
//     unsafe {
//         dma.devices[5].conblk_ad.write(mem_bus_addr(
//             cbs_ptr
//                 .offset(1)
//                 .cast::<u32>()
//                 .cast_mut()
//                 .expose_provenance() as u32,
//         ));
//         // clear END bit
//         dma.devices[5].cs.write(DMA_CS(0).with_end(true));
//         dma.devices[5].debug.write(7);
//     }
//
//     __dsb();
//
//     // start DMA
//     unsafe { dma.devices[5].cs.write(DMA_CS(1)) };
//
//     __dsb();
//     __dsb();
//
//     unsafe { smi.cs.modify(|r| r.with_start(true)) };
//
//     __dsb();
//
//     while dma.devices[5].txfr_len.read() > 0 {}
//     while dma.devices[5].cs.read().active() {}
//     // while !dma.devices[5].cs.read().end() {}
//
//     __dsb();
//
//     let _ = dma_cbs;
// }
