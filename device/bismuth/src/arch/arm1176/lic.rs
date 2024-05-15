//! Utilities for manipulating the Legacy Interrupt Controller.

use core::ptr::addr_of;
use crate::arch::arm1176::encoding::encode_branch;

/// Soft version of the Legacy Interrupt Controller Vector Table.
/// This data structure is not binary compatible with the LIC, but can be used to write an actual
/// table into memory ([`LICVectorTable::try_write_at`]).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct LICVectorTable {
    // offset 0x00, Supervisor mode
    pub reset: *const u32,
    // offset 0x04, Undefined mode
    pub undefined_instruction: *const u32,
    // offset 0x08, Supervisor mode
    pub software_interrupt: *const u32,
    // offset 0x0c, Abort mode
    pub prefetch_abort: *const u32,
    // offset 0x10, Abort mode
    pub data_abort: *const u32,
    // offset 0x18, IRQ mode
    pub irq: *const u32,
    // offset 0x1c, FIQ mode
    pub fiq: *const u32,
}

// From extern/lic.S
extern "C" {
    static __bis__lic_handle_reset_default : u32;
    static __bis__lic_handle_undefined_instruction_default : u32;
    static __bis__lic_handle_software_interrupt_default : u32;
    static __bis__lic_handle_prefetch_abort_default : u32;
    static __bis__lic_handle_data_abort_default : u32;
    static __bis__lic_handle_irq_default : u32;
    static __bis__lic_handle_fiq_default : u32;

    // TODO: SMCs and BKPTs don't go through the LIC.
    static __bis__lic_handle_smc_default : u32;
    static __bis__lic_handle_bkpt_default : u32;
}

impl LICVectorTable {
    pub fn new() -> Self {
        unsafe {
            Self {
                reset: addr_of!(__bis__lic_handle_reset_default),
                undefined_instruction: addr_of!(__bis__lic_handle_undefined_instruction_default),
                software_interrupt: addr_of!(__bis__lic_handle_software_interrupt_default),
                prefetch_abort: addr_of!(__bis__lic_handle_prefetch_abort_default),
                data_abort: addr_of!(__bis__lic_handle_data_abort_default),
                irq: addr_of!(__bis__lic_handle_irq_default),
                fiq: addr_of!(__bis__lic_handle_fiq_default),
            }
        }
    }
    pub unsafe fn try_write_at(&self, to: *mut u32) -> bool {
        let x : Option<()> = try {
            let enc_reset = encode_branch(to.byte_offset(0x0), self.reset)?;
            let enc_undef = encode_branch(to.byte_offset(0x4), self.undefined_instruction)?;
            let enc_swi = encode_branch(to.byte_offset(0x8), self.software_interrupt)?;
            let enc_pfa = encode_branch(to.byte_offset(0xc), self.prefetch_abort)?;
            let enc_dab = encode_branch(to.byte_offset(0x10), self.data_abort)?;
            let enc_irq = encode_branch(to.byte_offset(0x18), self.irq)?;
            let enc_fiq = encode_branch(to.byte_offset(0x1c), self.fiq)?;

            to.byte_offset(0x0).write(enc_reset);
            to.byte_offset(0x4).write(enc_undef);
            to.byte_offset(0x8).write(enc_swi);
            to.byte_offset(0xc).write(enc_pfa);
            to.byte_offset(0x10).write(enc_dab);
            to.byte_offset(0x18).write(enc_irq);
            to.byte_offset(0x1c).write(enc_fiq);

            ()
        };
        x.is_some()
    }
}