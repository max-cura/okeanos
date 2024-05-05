#![feature(panic_info_message)]
#![feature(str_from_raw_parts)]
// while i would love to use the core::arch::arm stuff
// it's not particularly compatible with armv6
// for example, armv6 dmb doesn't take any arguments, but core::arch::arm::__dmb requires one
#![no_std]
// we have need
#![allow(internal_features)]
#![feature(core_intrinsics)]

pub mod arch;
pub mod boot;
mod panic;
pub mod timing;
//pub mod critical_section;

#[no_mangle]
pub extern "C" fn __tlss_kernel_init() -> ! {
    boot::boot_init();

    loop {}
}

#[no_mangle]
pub extern fn __tlss_fast_reboot() -> ! {
    loop {}
}