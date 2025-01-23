#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(array_ptr_get)]
#![feature(pointer_is_aligned_to)]
#![no_std]

use crate::legacy::fmt::BOOT_UMSG_BUF;
use bcm2835_lpa::Peripherals;
use core::cell::UnsafeCell;
use core::panic::PanicInfo;
use okboot_common::INITIAL_BAUD_RATE;
use quartz::device::bcm2835::mini_uart;
use quartz::device::bcm2835::timing::delay_millis;

// pub mod arch;
mod buf;
pub mod legacy;
mod protocol;
mod stub;
// #[allow(dead_code)]
// mod sync;
pub mod timeouts;

#[repr(C, align(0x4000))]
pub struct TTBRegion(UnsafeCell<[u8; 0x4000]>);
unsafe impl Sync for TTBRegion {}
pub static TTB_REGION: TTBRegion = TTBRegion(UnsafeCell::new([0; 0x4000]));

#[unsafe(no_mangle)]
pub extern "C" fn __symbol_kstart() -> ! {
    // NOTE: It seems to be impractical/impossible to zero out the BSS in life-after-main, so we
    //       now do it in life-before-main (specifically, in _start in boot.S).
    // This is mostly because it is UB for the BSS to be uninitialized during AM execution, and also
    // because there is no way to get a pointer with provenance for the whole BSS section.

    let peripherals = unsafe { Peripherals::steal() };

    const _: () = {
        assert!(
            INITIAL_BAUD_RATE == 115200,
            "B115200_DIVIDER adjustment required"
        );
    };
    const B115200_DIVIDER: u16 = 270;
    mini_uart::muart1_init(
        &peripherals.GPIO,
        &peripherals.AUX,
        &peripherals.UART1,
        B115200_DIVIDER,
    );
    delay_millis(&peripherals.SYSTMR, 100);

    legacy_print_string_blocking!(&peripherals.UART1, "initializing MMU\n");

    unsafe {
        quartz::arch::arm1176::mmu::__init_mmu((*TTB_REGION.0.get()).as_mut_ptr().cast());
    }

    legacy_print_string_blocking!(&peripherals.UART1, "finished initializing MMU\n");

    unsafe {
        quartz::arch::arm1176::mmu::__set_mmu_enabled_features(
            quartz::arch::arm1176::mmu::MMUEnabledFeaturesConfig {
                dcache: Some(false),
                icache: Some(false),
                brpdx: Some(true),
            },
        )
    }
    legacy_print_string_blocking!(&peripherals.UART1, "MMU: -dcache -icache +brpdx\n");
    unsafe {
        quartz::arch::arm1176::mmu::__set_mmu_enabled_features(
            quartz::arch::arm1176::mmu::MMUEnabledFeaturesConfig {
                dcache: Some(true),
                icache: Some(true),
                brpdx: Some(true),
            },
        )
    }
    legacy_print_string_blocking!(&peripherals.UART1, "MMU: +dcache +icache +brpdx\n");

    protocol::run(&peripherals);

    legacy_print_string_blocking!(&peripherals.UART1, "protocol failure; restarting");

    __symbol_kreboot()
}

#[unsafe(no_mangle)]
pub extern "C" fn __symbol_kreboot() -> ! {
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
        legacy_print_string_blocking!(
            &peri.UART1,
            "[device]: Panic occurred at file '{}' line {}:\n",
            loc.file(),
            loc.line()
        );
    } else {
        legacy_print_string_blocking!(
            &peri.UART1,
            "[device]: Panic occurred at [unknown location]\n"
        );
    }
    let msg = info.message();
    use core::fmt::Write as _;
    let bub = unsafe { &mut *BOOT_UMSG_BUF.0.get() };
    bub.clear();
    if core::fmt::write(bub, format_args!("{}\n", msg)).is_err() {
        legacy_print_string_blocking!(
            &peri.UART1,
            "[device]: [failed to write message to format buffer]\n"
        );
    }
    if legacy::fmt::UartWrite::new(&peri.UART1)
        .write_str(bub.as_str())
        .is_err()
    {
        legacy_print_string_blocking!(&peri.UART1, "[device]: [failed to write message to uart]\n");
    }
    // } else {
    //     legacy_print_string_blocking!(&peri.UART1, "[device]: [no message]");
    // }
    legacy_print_string_blocking!(&peri.UART1, "[device]: rebooting.\n");

    __symbol_kreboot()
}
