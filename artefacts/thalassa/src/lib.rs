#![feature(ascii_char)]
#![feature(panic_info_message)]
#![no_std]

pub mod arch;
pub mod boot;
mod panic;

#[no_mangle]
pub extern "C" fn __tlss_kernel_init() -> ! {
    boot::boot_init();

    loop {}
}

#[no_mangle]
pub extern fn __tlss_fast_reboot() -> ! {
    loop {}
}