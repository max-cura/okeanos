#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(array_ptr_get)]
#![feature(pointer_is_aligned_to)]
#![no_std]

use crate::arch::mini_uart;
use crate::arch::timing::delay_millis;
use crate::legacy::fmt::BOOT_UMSG_BUF;
use crate::pmm_static::PMM;
use bcm2835_lpa::Peripherals;
use core::arch::asm;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::panic::PanicInfo;
use okboot_common::INITIAL_BAUD_RATE;

pub mod arch;
mod buf;
pub mod legacy;
mod protocol;
mod stub;
mod sync;
pub mod timeouts;

#[repr(C, align(0x4000))]
pub struct TTBRegion(UnsafeCell<[u8; 0x4000]>);
unsafe impl Sync for TTBRegion {}
pub static TTB_REGION: TTBRegion = TTBRegion(UnsafeCell::new([0; 0x4000]));
// optimization: this lets us go into BSS
mod pmm_static {
    use crate::arch::arm1176::pmm::PMM;
    use crate::arch::arm1176::sync::ticket::TicketLock;
    use crate::sync::once::OnceLockInit;
    use core::mem::size_of;

    static PMM_REGION: [u8; size_of::<PMM>()] = [0; size_of::<PMM>()];
    pub static PMM: OnceLockInit<
        TicketLock<&'static mut PMM>,
        fn() -> TicketLock<&'static mut PMM>,
    > = OnceLockInit::new(|| {
        TicketLock::new(unsafe {
            crate::arch::arm1176::pmm::pmm_init_at(
                core::ptr::NonNull::new(PMM_REGION.as_ptr().cast::<PMM>().cast_mut()).unwrap(),
            )
        })
    });
}
#[no_mangle]
pub extern "C" fn __symbol_kstart() -> ! {
    // NOTE: It seems to be impractical/impossible to zero out the BSS in life-after-main, so we
    //       now do it in life-before-main (specifically, in _start in boot.S).
    // This is mostly because it is UB for the BSS to be uninitialized during AM execution, and also
    // because there is no way to get a pointer with provenance for the whole BSS section.

    let peripherals = unsafe { Peripherals::steal() };
    mini_uart::muart1_init(&peripherals.GPIO, &peripherals.AUX, &peripherals.UART1, 270);
    delay_millis(&peripherals.SYSTMR, 100);

    {
        // check BSS
        let base = &raw const stub::__symbol_bss_start__;
        let end = &raw const stub::__symbol_bss_end__;
        let mut p = base.as_ptr();
        let end = end.as_ptr();
        while p < end {
            let b = unsafe { p.read_volatile() };
            if unsafe { b } != 0 {
                legacy_print_string_blocking!(&peripherals.UART1, "bad BSS byte at {p:#?}: {b}\n",);
            }
            p = unsafe { p.add(1) };
        }
    }

    legacy_print_string_blocking!(&peripherals.UART1, "initializing MMU\n");

    unsafe {
        arch::arm1176::mmu::__init_mmu((*TTB_REGION.0.get()).as_mut_ptr().cast());
    }

    legacy_print_string_blocking!(&peripherals.UART1, "finished initializing MMU\n");

    unsafe {
        arch::arm1176::mmu::__set_mmu_enabled_features(
            arch::arm1176::mmu::MMUEnabledFeaturesConfig {
                dcache: Some(false),
                icache: Some(false),
                brpdx: Some(true),
            },
        )
    }
    legacy_print_string_blocking!(&peripherals.UART1, "MMU: -dcache -icache +brpdx\n");
    let mut pmm = PMM.get().lock();
    unsafe {
        (&mut pmm).initialize_once(&[(
            0 as *mut u8,
            core::ptr::addr_of!(crate::stub::__symbol_exec_end__)
                .cast_mut()
                .cast(),
        )])
    }
    legacy_print_string_blocking!(&peripherals.UART1, "built PMM\n");
    unsafe {
        arch::arm1176::mmu::__set_mmu_enabled_features(
            arch::arm1176::mmu::MMUEnabledFeaturesConfig {
                dcache: Some(true),
                icache: Some(true),
                brpdx: Some(true),
            },
        )
    }
    legacy_print_string_blocking!(&peripherals.UART1, "MMU: +dcache +icache +brpdx\n");

    {
        // let mut p = (&raw const stub::__symbol_exec_end__).addr() as *const u8;
        // let end = 0x2000_0000 as *const [u8; 0];
        // let end = end.as_ptr();
        // while p < end {
        //     let b = unsafe { p.read_volatile() };
        //     if unsafe { b } != 0 {
        //         legacy_print_string_blocking!(&peripherals.UART1, "{p:#?}: {b}\n",);
        //     }
        //     p = unsafe { p.add(1) };
        // }
    }

    peripherals
        .GPIO
        .gpfsel2()
        .modify(|_, w| w.fsel27().output());

    legacy_print_string_blocking!(&peripherals.UART1, "FSEL27 done\n");

    let mut sp: u32;
    unsafe {
        asm!(
        "mov {t}, sp",
        "wfe",
        t = out(reg) sp
        );
    }
    legacy_print_string_blocking!(&peripherals.UART1, "SP={sp:08x}\n");
    protocol::run(&peripherals);

    // legacy_print_string_blocking!(&peripherals.UART1, "protocol failure; restarting");

    __symbol_kreboot()
}

#[no_mangle]
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
