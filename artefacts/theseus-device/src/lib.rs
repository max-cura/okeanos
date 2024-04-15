#![feature(panic_info_message)]
#![no_std]

use core::arch::asm;
use bcm2835_lpa::{Peripherals};
use theseus_common::INITIAL_BAUD_RATE;
use crate::fmt::UartWrite;
use core::fmt::Write as _;

pub mod fmt;
pub mod uart1;
pub mod cobs;
pub mod delay;
mod download;
mod legacy;

fn data_synchronization_barrier() {
    unsafe {
        asm!(
            // DSB is marked as SBZ, Should Be Zero.
            // See: arm1176.pdf 3-70, 3-71
            "mcr p15,0,{tmp},c7,c10,4",
            tmp = in(reg) 0,
        );
    }
}


pub(crate) static mut DID_LOAD_UART : bool = false;
pub(crate) static mut IN_THESEUS : bool = false;

// problem: wire format is MessageContent -> postcard -> COBS

extern "C" {
    static __theseus_code_start__: u8;
    static __theseus_prog_end__: u8;

    static _relocation_stub: u8;
    static _relocation_stub_end: u8;
}
extern "C" {
    static mut __theseus_bss_start__ : u8;
    static __theseus_bss_end__ : u8;
}
fn bss_zero() {
    unsafe {
        let start = core::ptr::addr_of_mut!(__theseus_bss_start__);
        let end = core::ptr::addr_of!(__theseus_bss_end__);
        let len = end.offset_from(start) as usize;
        let bytes = core::slice::from_raw_parts_mut(start, len);
        bytes.iter_mut().for_each(|b| *b = 0x00);
    }
}

#[no_mangle]
pub extern "C" fn __theseus_init() {
    bss_zero();

    let peripherals = unsafe { Peripherals::steal() };
    uart1::uart1_init(&peripherals.GPIO, &peripherals.AUX, &peripherals.UART1, INITIAL_BAUD_RATE);
    unsafe { DID_LOAD_UART = true };
    let mut ser = UartWrite::new(&peripherals.UART1);
    boot_umsg!(ser, "[theseus-device]: loaded UART1, entering legacy SU-BOOT compat mode");

    // unsafe for extern static
    let b = unsafe { core::ptr::addr_of!(__theseus_code_start__) };
    let c = unsafe { core::ptr::addr_of!(__theseus_prog_end__) };
    boot_umsg!(ser, "[theseus-device]: currently loaded at [{b:#?}..{c:#?}]");

    loop {
        download::download(&mut ser, &peripherals.UART1, &peripherals.SYSTMR);

        boot_umsg!(ser, "[theseus-device]: fell out of download::download. Rebooting.");

        boot_umsg!(ser, "[theseus-device]: sike no reboot");
        // TODO reboot
    }
}

#[no_mangle]
pub extern "C" fn __theseus_reboot() -> ! {
    if unsafe { DID_LOAD_UART } {
        let peripherals = unsafe { Peripherals::steal() };
        let mut ser = UartWrite::new(&peripherals.UART1);
        boot_umsg!(ser, "[theseus-device]: rebooting");

        boot_umsg!(ser, "[theseus-device]: reboot functionality not yet implemented, halting.");
    }
    // TODO reboot
    loop {}
}

#[panic_handler]
fn panic(info: &::core::panic::PanicInfo) -> ! {
    if unsafe { DID_LOAD_UART } {
        let peripherals = unsafe { Peripherals::steal() };
        let mut ser = UartWrite::new(&peripherals.UART1);
        if let Some(loc) = info.location() {
            boot_umsg!(ser, "[theseus-device]: Panic occurred at file '{}' line {}:", loc.file(), loc.line());
        } else {
            boot_umsg!(ser, "[theseus-device]: Panic occurred at [unknown location]");
        }
        if let Some(msg) = info.message() {
            if core::fmt::write(&mut ser, *msg).is_err() {
                boot_umsg!(ser, "[theseus-device]: [failed to write message]");
            }
        } else {
            boot_umsg!(ser, "[theseus-device]: [no message]");
        }
        boot_umsg!(ser, "[theseus-device]: rebooting.")
    }

    __theseus_reboot()
}
