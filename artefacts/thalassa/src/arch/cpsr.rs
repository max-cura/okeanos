use core::arch::asm;

pub unsafe fn unsafe_try_disable_irqs() -> Result<(), ()> {
    let t1 : u32;
    asm!(
        "mrs {t1}, cpsr",
        "orr {tmp}, {t1}, #(1 << 7)",
        "msr cpsr, {tmp}",
        t1 = out(reg) t1,
        tmp = out(reg) _,
    );
    if (t1 & 7) == 0 {
        // interrupts were enabled but are now
        Ok(())
    } else {
        // interrupts already disabled
        Err(())
    }
}

pub unsafe fn unsafe_try_enable_irs() -> Result<(), ()> {
    let t1 : u32;
    asm!(
        "mrs {t1}, cpsr",
        "and {tmp}, {t1}, #(~(1 << 7))",
        "msr cpsr, {tmp}",
        t1 = out(reg) t1,
        tmp = out(reg) _,
    );
    if (t1 & 7) == 0 {
        // interrupts were already enabled
        Err(())
    } else {
        // interrupts disabled and are now enabled
        Ok(())
    }
}