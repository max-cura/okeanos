#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(pointer_is_aligned_to)]
#![feature(array_try_map)]
#![feature(vec_into_raw_parts)]
#![feature(box_vec_non_null)]
#![feature(thread_local)]
#![feature(box_into_inner)]
#![feature(slice_ptr_get)]
#![feature(ptr_as_ref_unchecked)]
#![feature(box_as_ptr)]
#![no_std]

extern crate alloc;

#[global_allocator]
static HEAP: embedded_alloc::TlsfHeap = embedded_alloc::TlsfHeap::empty();

mod app;
mod exceptions;
pub mod fmt;
pub mod int;
pub mod thread;

use crate::fmt::Uart1WriteProxy;
use bcm2835_lpa::Peripherals;
use core::cell::UnsafeCell;
use core::panic::PanicInfo;
use critical_section::RawRestoreState;
use lock_api::RawMutex;
use quartz::arch::arm1176::__dsb;
use quartz::arch::arm1176::sync::ticket::RawTicketLock;
use quartz::device::bcm2835::mini_uart;

#[macro_export]
macro_rules! steal_println {
    ($($e:tt)*) => {
        let peri = unsafe{::bcm2835_lpa::Peripherals::steal()};
        $crate::uart1_println!(&peri.UART1, $($e)*)
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn __aeabi_unwind_cpp_pr0() {}

#[unsafe(no_mangle)]
pub extern "C" fn _interrupt_svc(r0: u32, r1: u32, r2: u32, swi_immed: u32, r3: u32) {
    let peri = unsafe { Peripherals::steal() };
    uart1_println!(
        &peri.UART1,
        "_interrupt_svc({swi_immed:08x}) r0={r0:08x} r1={r1:08x} r2={r2:08x} r3={r3:08x}"
    );
}
#[unsafe(no_mangle)]
pub extern "C" fn _interrupt_irq() {
    let peri = unsafe { Peripherals::steal() };
    __dsb();
    let timer_pending = peri.LIC.basic_pending().read().timer().bit_is_set();
    __dsb();
    if timer_pending {
        let _tim_val = 0x2000b404 as *mut u32;
        let tim_irq_clr = 0x2000b40c as *mut u32;
        __dsb();
        unsafe {
            tim_irq_clr.write_volatile(1);
        }
        __dsb();
    }
}

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
    mini_uart::muart1_init(&peripherals.GPIO, &peripherals.AUX, &peripherals.UART1, 270);
    quartz::device::bcm2835::timing::delay_millis(&peripherals.SYSTMR, 100);

    uart1_println!(&peripherals.UART1, "[bis] finished initializing UART1");

    unsafe {
        #[repr(C, align(0x4000))]
        pub struct TTBRegion(UnsafeCell<[u8; 0x4000]>);
        unsafe impl Sync for TTBRegion {}
        pub static TTB_REGION: TTBRegion = TTBRegion(UnsafeCell::new([0; 0x4000]));
        quartz::arch::arm1176::mmu::__init_mmu((*TTB_REGION.0.get()).as_mut_ptr().cast());
    }

    uart1_println!(&peripherals.UART1, "[bis] finished initializing MMU");

    unsafe { HEAP.init(0x1000_0000, 0x1000_0000) };

    uart1_println!(&peripherals.UART1, "[bis] finished initializing HEAP");

    app::debug::run();

    steal_println!("et fini");

    __symbol_kreboot()
}

#[unsafe(no_mangle)]
pub extern "C" fn __symbol_kreboot() -> ! {
    let peripherals = unsafe { Peripherals::steal() };
    uart1_println!(&peripherals.UART1, "[bis] rebooting");
    quartz::device::bcm2835::watchdog::restart(&peripherals.PM);
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

struct MyCriticalSection;
critical_section::set_impl!(MyCriticalSection);

static CRITICAL_SECTION_LOCK: RawTicketLock = RawTicketLock::INIT;

unsafe impl critical_section::Impl for MyCriticalSection {
    unsafe fn acquire() -> RawRestoreState {
        // TODO
        // let cpsr_orig = crate::arch::arm1176::cpsr::__read_cpsr();
        // __write_cpsr(cpsr_orig.with_disable_irq(true));
        // CRITICAL_SECTION_LOCK.lock();
        0
    }

    unsafe fn release(_token: RawRestoreState) {
        // unsafe { CRITICAL_SECTION_LOCK.unlock() };
        // TODO
    }
}
