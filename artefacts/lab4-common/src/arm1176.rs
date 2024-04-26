use core::arch::asm;

pub fn __dsb() {
    unsafe {
        asm!(
            // DSB is marked as SBZ, Should Be Zero.
            // See: arm1176.pdf 3-70, 3-71
            "mcr p15,0,{tmp},c7,c10,4",
            tmp = in(reg) 0,
        );
    }
}
