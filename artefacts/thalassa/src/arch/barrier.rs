use core::arch::asm;

// TODO: CHECK SAFETY

/// Also called Drain Write Buffer/Data Write Barrier.
/// This function returns when all explicit memory transactions occurring in program order before
/// this have completed. No instructions occurring in program order after the Data Synchronization
/// Barrier complete, or change the interrupt masks, until this instruction completes.
/// As a special case of that, no explicit memory transactions occurring in program order after this
/// instruction are started until this instruction complete.
/// See: arm1176.pdf 6-25, 3-83:84
#[inline]
pub fn data_synchronization_barrier() {
    unsafe {
        asm!(
        // DSB is marked as SBZ, Should Be Zero.
        // See: arm1176.pdf 3-70, 3-71
        "mcr p15,0,{tmp},c7,c10,4",
        tmp = in(reg) 0,
        );
    }
}

/// From the TRM:
/// > This memory barrier ensures that all explicit memory transactions occurring in program order
/// > before this instruction are completed. No explicit memory transactions occurring in program
/// > order after this instruction are started until this instruction completes. Other instructions
/// > can complete out of order with the Data Memory Barrier instruction.
/// See: arm1176.pdf 6-25
#[inline]
pub fn data_memory_barrier() {
    unsafe {
        asm!(
        // DMB is marked as SBZ, Should Be Zero.
        // See: arm1176.pdf 3-70, 3-71
        "mcr p15,0,{tmp},c7,c10,5",
        tmp = in(reg) 0,
        )
    }
}

#[inline]
pub fn wait_for_event() {
    unsafe {
        asm!("wfe")
    }
}
#[inline]
pub fn set_event() {
    unsafe {
        asm!("sev")
    }
}

/// Compiler will refuse to reorder around this call.
#[inline]
pub fn compiler_fence() {
    unsafe { asm!("") }
}