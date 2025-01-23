#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(pointer_is_aligned_to)]
#![feature(array_try_map)]
#![no_std]

pub mod fmt;
pub mod int;

use crate::int::{_interrupt_irq, InterruptMode, OperatingMode, X};
use bcm2835_lpa::Peripherals;
use core::arch::asm;
use core::cell::UnsafeCell;
use core::panic::PanicInfo;
use quartz::arch::arm1176::__dsb;
use quartz::device::bcm2835::timing::delay_millis;

#[derive(Debug, Copy, Clone)]
#[repr(C, align(32))]
struct AlignedJumpTable {
    inner: [u32; 8],
}
#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
struct UnsafeSync<T>(T);
unsafe impl<T> Sync for UnsafeSync<T> {}
static JUMP_TABLE: UnsafeSync<UnsafeCell<AlignedJumpTable>> =
    UnsafeSync(UnsafeCell::new(AlignedJumpTable { inner: [0; 8] }));

#[unsafe(no_mangle)]
pub extern "C" fn __symbol_kstart() -> ! {
    let peripherals = unsafe { Peripherals::steal() };
    peripherals
        .GPIO
        .gpfsel2()
        .modify(|_, w| w.fsel27().output());
    unsafe {
        peripherals
            .GPIO
            .gpclr0()
            .write_with_zero(|w| w.bits(0xffff_ffff));
    }
    quartz::device::bcm2835::mini_uart::muart1_init(
        &peripherals.GPIO,
        &peripherals.AUX,
        &peripherals.UART1,
        270,
    );
    quartz::device::bcm2835::timing::delay_millis(&peripherals.SYSTMR, 100);
    uart1_println!(&peripherals.UART1, "[bis] starting");

    if let Err(e) = unsafe { int::install_interrupts(JUMP_TABLE.0.get().cast()) } {
        uart1_println!(
            &peripherals.UART1,
            "[bis] failed to install interrupt vector: {e:?}"
        );
        __symbol_kreboot();
    }

    uart1_println!(&peripherals.UART1, "[bis] installed interrupt vector");

    // unsafe {
    //     int::init_stack_for_mode(OperatingMode::System, 0x0c00_0000);
    // }
    // uart1_println!(&peripherals.UART1, "[bis] installed stack for mode: SYSTEM");
    // unsafe {
    //     core::arch::asm!("cps #0b10000");
    // }
    // uart1_println!(&peripherals.UART1, "[bis] about to SWI");
    // let mut addr: u32;
    // unsafe {
    //     core::arch::asm!("mov {t0}, pc", t0 = out(reg) addr);
    // }
    // uart1_println!(&peripherals.UART1, "[bis] pre-SWI address is {addr:08x}");
    // unsafe {
    //     // now in user mode
    //     core::arch::asm!(
    //         "swi #666",
    //         inout("r0") 0x1111 => _,
    //         inout("r1") 0x2222 => _,
    //         inout("r2") 0x3333 => _,
    //         inout("r3") 0x4444 => _,
    //     );
    // }
    // uart1_println!(&peripherals.UART1, "[bis] returned from SWI");

    let tim_val = 0x2000b404 as *mut u32;
    uart1_println!(&peripherals.UART1, "[bis] enabling timer");
    unsafe {
        int::init_stack_for_mode(OperatingMode::IRQ, 0x0d00_0000);

        __dsb();
        peripherals.LIC.disable_1().write(|w| w.bits(0xffff_ffff));
        peripherals.LIC.disable_2().write(|w| w.bits(0xffff_ffff));
        // peripherals
        //     .LIC
        //     .disable_basic()
        //     .write(|w| w.bits(0xffff_ffff));
        __dsb();
        peripherals
            .LIC
            .enable_basic()
            .write(|w| w.timer().set_bit());
        __dsb();

        // timer setup
        let tim_load = 0x2000b400 as *mut u32;
        let tim_ctrl = 0x2000b408 as *mut u32;
        let tim_irq_clr = 0x2000b40c as *mut u32;
        // APB_clock / (pre_scaler + 1)
        // APB_clock may or may not be the core clock at 250MHz
        // tim_pre.write_volatile();
        // for 16-bit counter mode
        tim_load.write_volatile(0x100);
        // // let tim_pre = tim.add(0x41c);
        // tim_ctrl.write_volatile(0x3e00_0000);
        // unsure what value; just says "when writing" so maybe anything?
        // tim_irq_clr.write_volatile(0);
        tim_ctrl.write_volatile(
            // (0b1 << 1) // 32-bit counters
            //     | (0b01 << 2) // pre scale is clock/16 - not sure which prescale this is
            //     | (0b1 << 5) // enable timer interrupt
            //     | (0b1 << 7), // enable timer
            (tim_ctrl.read_volatile() & !0b11_1111_1111) | 0b11_1110_0110,
        );

        __dsb();
    }

    unsafe {
        // int::set_enabled_interrupts(InterruptMode::IrqOnly);
        asm!("cpsie i");
    }

    // loop {}

    delay_millis(&peripherals.SYSTMR, 5000);

    unsafe {
        asm!("cpsid i");
        // int::set_enabled_interrupts(InterruptMode::Neither);
    }

    uart1_println!(&peripherals.UART1, "[bis] finished waiting, IRQs disabled");
    uart1_println!(&peripherals.UART1, "[bis] X={:?}", unsafe {
        X.0.get().read_volatile()
    });

    __symbol_kreboot()
}

#[unsafe(no_mangle)]
pub extern "C" fn __symbol_kreboot() -> ! {
    let peripherals = unsafe { Peripherals::steal() };
    uart1_println!(&peripherals.UART1, "[bis] rebooting");
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    __symbol_kreboot()
}
