#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(pointer_is_aligned_to)]
#![feature(array_try_map)]
#![no_std]

pub mod fmt;
mod gpio;
pub mod int;

use crate::fmt::Uart1WriteProxy;
use crate::int::{OperatingMode, TABLE};
use bcm2835_lpa::Peripherals;
use core::arch::asm;
use core::cell::UnsafeCell;
use core::panic::PanicInfo;
use quartz::arch::arm1176::__dsb;
use quartz::device::bcm2835::mini_uart;
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
#[unsafe(no_mangle)]
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

    uart1_println!(&peripherals.UART1, "initializing MMU\n");

    unsafe {
        #[repr(C, align(0x4000))]
        pub struct TTBRegion(UnsafeCell<[u8; 0x4000]>);
        unsafe impl Sync for TTBRegion {}
        pub static TTB_REGION: TTBRegion = TTBRegion(UnsafeCell::new([0; 0x4000]));
        // quartz::arch::arm1176::mmu::__init_mmu((*TTB_REGION.0.get()).as_mut_ptr().cast());
    }

    uart1_println!(&peripherals.UART1, "finished initializing MMU\n");

    uart1_println!(&peripherals.UART1, "[bis] starting");

    if let Err(e) = unsafe { int::install_interrupts(JUMP_TABLE.0.get().cast()) } {
        uart1_println!(
            &peripherals.UART1,
            "[bis] failed to install interrupt vector: {e:?}"
        );
        __symbol_kreboot();
    }

    uart1_println!(&peripherals.UART1, "[bis] installed interrupt vector");

    unsafe {
        int::init_stack_for_mode(OperatingMode::System, 0x0c00_0000);
    }
    uart1_println!(&peripherals.UART1, "[bis] installed stack for mode: SYSTEM");
    unsafe {
        core::arch::asm!("cps #0b10000");
    }
    uart1_println!(&peripherals.UART1, "[bis] about to SWI");
    let mut addr: u32;
    unsafe {
        core::arch::asm!("mov {t0}, pc", t0 = out(reg) addr);
    }
    uart1_println!(&peripherals.UART1, "[bis] pre-SWI address is {addr:08x}");
    __dsb();
    unsafe {
        // now in user mode
        core::arch::asm!(
            "swi #0x666",
            inout("r0") 0x1111 => _,
            inout("r1") 0x2222 => _,
            inout("r2") 0x3333 => _,
            inout("r3") 0x4444 => _,
        );
    }
    uart1_println!(&peripherals.UART1, "[bis] returned from SWI");

    // __dsb();
    // // unsafe { asm!("wfe") };
    // let mut i: usize = 0x1000_0000;
    // while i < 0x2000_0000 {
    //     if i % 0x100_0000 == 0 {
    //         uart1_println!(&peripherals.UART1, "[bis] i={i:08x}");
    //     }
    //     unsafe {
    //         // core::ptr::with_exposed_provenance_mut::<u32>(i).write_volatile(0);
    //         // (i as *mut u32).write_volatile(0);
    //         asm!(
    //             "str {z}, [{a}], #4",
    //             a = inout(reg) i,
    //             z = in(reg) 0,
    //         );
    //     }
    //     // if i % 0x1000 == 0 {
    //     //     uart1_println!(&peripherals.UART1, "[bis] i={i:08x}");
    //     // }
    //     // i += 4;
    // }
    // // unsafe { asm!("wfe") };
    // __dsb();
    //
    // let tim_val = 0x2000b404 as *mut u32;
    // uart1_println!(&peripherals.UART1, "[bis] enabling timer");
    // unsafe {
    //     int::init_stack_for_mode(OperatingMode::IRQ, 0x0d00_0000);
    //
    //     __dsb();
    //     peripherals.LIC.disable_1().write(|w| w.bits(0xffff_ffff));
    //     peripherals.LIC.disable_2().write(|w| w.bits(0xffff_ffff));
    //     // peripherals
    //     //     .LIC
    //     //     .disable_basic()
    //     //     .write(|w| w.bits(0xffff_ffff));
    //     __dsb();
    //     peripherals
    //         .LIC
    //         .enable_basic()
    //         .write(|w| w.timer().set_bit());
    //     __dsb();
    //
    //     // timer setup
    //     let tim_load = 0x2000b400 as *mut u32;
    //     let tim_ctrl = 0x2000b408 as *mut u32;
    //     let tim_irq_clr = 0x2000b40c as *mut u32;
    //     // APB_clock / (pre_scaler + 1)
    //     // APB_clock may or may not be the core clock at 250MHz
    //     // tim_pre.write_volatile();
    //     // for 16-bit counter mode
    //     tim_load.write_volatile(0x10);
    //     // // let tim_pre = tim.add(0x41c);
    //     // tim_ctrl.write_volatile(0x3e00_0000);
    //     // unsure what value; just says "when writing" so maybe anything?
    //     // tim_irq_clr.write_volatile(0);
    //     tim_ctrl.write_volatile(
    //         // (0b1 << 1) // 32-bit counters
    //         //     | (0b01 << 2) // pre scale is clock/16 - not sure which prescale this is
    //         //     | (0b1 << 5) // enable timer interrupt
    //         //     | (0b1 << 7), // enable timer
    //         (tim_ctrl.read_volatile() & !0b11_1111_1111) | 0b11_1110_0110,
    //     );
    //
    //     __dsb();
    // }
    //
    // unsafe {
    //     // int::set_enabled_interrupts(InterruptMode::IrqOnly);
    //     asm!("cpsie i");
    // }
    //
    // // loop {}
    //
    // delay_millis(&peripherals.SYSTMR, 1000);
    //
    // unsafe {
    //     asm!("cpsid i");
    //     // int::set_enabled_interrupts(InterruptMode::Neither);
    // }
    //
    // uart1_println!(&peripherals.UART1, "[bis] finished waiting, IRQs disabled");
    // let addrs = unsafe { (&*TABLE.0.get()).addrs.as_ptr().cast_mut() };
    // let next = unsafe { &raw const (&*TABLE.0.get()).next }.cast_mut();
    // let fencepost = unsafe { next.read_volatile() };
    // for i in 0..fencepost {
    //     let addr = unsafe { addrs.offset(i as isize).read_volatile() };
    //     let shadow: *const u32 = core::ptr::with_exposed_provenance(addr as usize | 0x1000_0000);
    //     let count = unsafe { shadow.read_volatile() };
    //
    //     uart1_println!(&peripherals.UART1, "{addr:08x}: {count:08x}");
    // }
    // // uart1_println!(&peripherals.UART1, "[bis] HT={:?}", unsafe {});

    __symbol_kreboot()
}

#[unsafe(no_mangle)]
pub extern "C" fn __symbol_kreboot() -> ! {
    let peripherals = unsafe { Peripherals::steal() };
    uart1_println!(&peripherals.UART1, "[bis] rebooting");
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // TODO: refactor

    let peri = unsafe { Peripherals::steal() };

    // peri.GPIO.gpfsel2().modify(|_, w| w.fsel27().output());
    // unsafe { peri.GPIO.gpset0().write_with_zero(|w| w.set27().set_bit()) };

    mini_uart::muart1_init(&peri.GPIO, &peri.AUX, &peri.UART1, 270);

    if let Some(loc) = info.location() {
        uart1_println!(
            &peri.UART1,
            "[device]: Panic occurred at file '{}' line {}:\n",
            loc.file(),
            loc.line()
        );
    } else {
        uart1_println!(
            &peri.UART1,
            "[device]: Panic occurred at [unknown location]\n"
        );
    }
    let msg = info.message();
    use core::fmt::Write as _;
    let mut proxy = Uart1WriteProxy::new(&peri.UART1);
    if core::fmt::write(&mut proxy, format_args!("{}\n", msg)).is_err() {
        uart1_println!(
            &peri.UART1,
            "[device]: [failed to write message to format buffer]\n"
        );
    }
    uart1_println!(&peri.UART1, "[device]: rebooting.\n");

    __symbol_kreboot()
}
