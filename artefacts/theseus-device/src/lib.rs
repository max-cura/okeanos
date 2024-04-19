#![feature(panic_info_message)]
#![feature(error_in_core)]
#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(decl_macro)]
#![no_std]

pub mod arm1176;
pub mod stub;
pub mod timing;

pub mod muart;

pub mod legacy;
pub mod theseus;

mod reactor;

#[no_mangle]
pub extern "C" fn __theseus_init() {
    stub::zero_stub_bss();

    loop {
        reactor::run();
    }
}

#[no_mangle]
pub extern "C" fn __theseus_reboot() -> ! {
    // TODO reboot
    loop {}
}

#[panic_handler]
fn panic(info: &::core::panic::PanicInfo) -> ! {
    __theseus_reboot()
}
