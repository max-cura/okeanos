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

// extern crate alloc;

extern crate alloc;

#[allow(non_upper_case_globals)]
pub const KiB: usize = 1024;
#[allow(non_upper_case_globals)]
pub const MiB: usize = 1024 * KiB;

use core::fmt::Write;

pub mod arch;
pub mod boot;
pub mod data;
pub mod kalloc;
pub mod muart;
pub mod peripherals;
pub mod symbols;
pub mod sync;

extern "C" {
    fn __bis__main();
}

#[no_mangle]
pub extern "C" fn __symbol_kstart__() {
    boot::boot();

    unsafe { __bis__main() }

    __symbol_reboot__();
}

#[no_mangle]
pub extern "C" fn __symbol_reboot__() -> ! {
    // TODO

    loop {}
}

#[panic_handler]
pub fn panic(info: &::core::panic::PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        uart1_sendln_bl!(
            "[device]: Panic occurred at file '{}' line {}:",
            loc.file(),
            loc.line()
        );
    } else {
        uart1_sendln_bl!("[device]: Panic occurred at [unknown location]");
    }
    if let Some(msg) = info.message() {
        if muart::UartBlockingFmt.write_fmt(*msg).is_err() {
            uart1_sendln_bl!("[device]: [failed to write message to UART]");
        }
    } else {
        uart1_sendln_bl!("[device]: [no message]");
    }
    uart1_sendln_bl!("[device]: rebooting.");

    __symbol_reboot__();
}

use crate::arch::arm1176::cpsr::__write_cpsr;
use crate::sync::ticket::RawTicketLock;
use critical_section::RawRestoreState;
use lock_api::RawMutex;

struct MyCriticalSection;
critical_section::set_impl!(MyCriticalSection);

static CRITICAL_SECTION_LOCK: RawTicketLock = RawTicketLock::INIT;

unsafe impl critical_section::Impl for MyCriticalSection {
    unsafe fn acquire() -> RawRestoreState {
        // TODO
        // let cpsr_orig = crate::arch::arm1176::cpsr::__read_cpsr();
        // __write_cpsr(cpsr_orig.with_disable_irq(true));
        CRITICAL_SECTION_LOCK.lock();
        0
    }

    unsafe fn release(token: RawRestoreState) {
        CRITICAL_SECTION_LOCK.unlock();
        // TODO
    }
}
