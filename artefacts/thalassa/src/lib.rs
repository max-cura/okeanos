#![no_std]
// #![no_main]

#[panic_handler]
fn panic(_info: &::core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn rs_add(a: i32, b: i32) -> i32 {
    a.overflowing_add(b).0
}

#[no_mangle]
pub extern "C" fn _tlss_kernel_init() -> ! {
    unsafe { core::arch::asm!(
        "mov sp, #0xf000"
    ); }
    loop {}
}

#[no_mangle]
pub extern fn _tlss_fast_reboot() -> ! {
    unsafe { core::arch::asm!(
        "mov sp, #0x8000"
    ); }
    loop {}
}