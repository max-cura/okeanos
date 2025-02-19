use crate::int::{InterruptMode, OperatingMode};
use thiserror::Error;

quartz::cpreg!(vector_base_address, p15, 0, c12, c0, 0);

#[derive(Debug, Error, Copy, Clone)]
pub enum ExceptionError {
    #[error("cannot produce jump instruction from {from:08x} to {to:08x}")]
    TooFar { from: usize, to: usize },
    #[error("vector table address must be 32-byte aligned")]
    VectorTableAlignment,
    #[error("vector table jump target address must be 4-byte aligned")]
    JumpTargetAlignment,
    #[error("destination space too small")]
    DestinationTooSmall,
}

pub type CodePtr = *const [u32; 0];

pub enum FiqAction {
    Jump(CodePtr),
    Code(*mut [u32]),
}

pub struct VectorBuilder {
    maps: [CodePtr; 7],
    fiq: FiqAction,
}

impl VectorBuilder {
    pub fn new() -> Self {
        Self {
            maps: [
                unsafe { &raw const defaults::_landing_pad_reset },
                unsafe { &raw const defaults::_landing_pad_undef },
                unsafe { &raw const defaults::_landing_pad_svc },
                unsafe { &raw const defaults::_landing_pad_pabt },
                unsafe { &raw const defaults::_landing_pad_dabt },
                unsafe { &raw const defaults::_landing_pad_none },
                unsafe { &raw const defaults::_landing_pad_irq },
            ],
            fiq: FiqAction::Jump(unsafe { &raw const defaults::_landing_pad_fiq }),
        }
    }
    pub fn set_undefined_instruction_handler(mut self, action: CodePtr) -> Self {
        self.maps[1] = action;
        self
    }
    pub fn set_syscall_handler(mut self, action: CodePtr) -> Self {
        self.maps[2] = action;
        self
    }
    pub fn set_prefetch_abort_handler(mut self, action: CodePtr) -> Self {
        self.maps[3] = action;
        self
    }
    pub fn set_data_abort_handler(mut self, action: CodePtr) -> Self {
        self.maps[4] = action;
        self
    }
    pub fn set_irq_handler(mut self, action: CodePtr) -> Self {
        self.maps[6] = action;
        self
    }
    pub fn set_fiq_handler(mut self, fiq: FiqAction) -> Self {
        self.fiq = fiq;
        self
    }
    pub unsafe fn install(self, dst: *mut [u32]) -> Result<(), ExceptionError> {
        let dst_base = dst.as_mut_ptr();
        if !dst_base.is_aligned_to(0x20) {
            return Err(ExceptionError::VectorTableAlignment);
        }
        let dst_base_addr = dst_base.expose_provenance();
        let jump_target_addresses = self.maps.map(|p| p.addr());
        let jump_source_addresses =
            [0x0, 0x4, 0x8, 0xc, 0x10, 0x14, 0x18, 0x1c].map(|offset| dst_base_addr + offset);
        let encodings = [0, 1, 2, 3, 4, 5, 6].try_map(|i| {
            encode_relative_jump(jump_target_addresses[i], jump_source_addresses[i])
        })?;
        let mut fiq_buffer;
        let (fiq_ptr_src, fiq_words) = match self.fiq {
            FiqAction::Jump(fiq_jump_target) => {
                let fiq_jump_source = jump_source_addresses[7];
                fiq_buffer = encode_relative_jump(fiq_jump_target.addr(), fiq_jump_source)?;
                (&raw const fiq_buffer, 1usize)
            }
            FiqAction::Code(fiq_code_slice) => (
                fiq_code_slice.as_mut_ptr().cast_const(),
                fiq_code_slice.len(),
            ),
        };
        if dst.len() < 7 + fiq_words {
            return Err(ExceptionError::DestinationTooSmall);
        }
        let mut fiq_source_iter = 0;
        let source_iter = encodings.into_iter().chain(core::iter::from_fn(|| {
            if fiq_source_iter == fiq_words {
                return None;
            } else {
                let word = unsafe { fiq_ptr_src.offset(fiq_source_iter as isize).read_volatile() };
                fiq_source_iter += 1;
                Some(word)
            }
        }));

        let save = super::int::set_enabled_interrupts(InterruptMode::Neither);
        {
            for (idx, word) in source_iter.enumerate() {
                unsafe { dst_base.offset(idx as isize).write_volatile(word) };
            }
            unsafe { vector_base_address::write(dst_base_addr) };
        }
        super::int::set_enabled_interrupts(save);

        Ok(())
    }
}

mod defaults {
    unsafe extern "C" {
        pub static _landing_pad_svc: [u32; 0];
        pub static _landing_pad_smc: [u32; 0];
        pub static _landing_pad_undef: [u32; 0];
        pub static _landing_pad_pabt: [u32; 0];
        pub static _landing_pad_fiq: [u32; 0];
        pub static _landing_pad_irq: [u32; 0];
        pub static _landing_pad_dabt: [u32; 0];
        pub static _landing_pad_reset: [u32; 0];
        pub static _landing_pad_bkpt: [u32; 0];
        pub static _landing_pad_none: [u32; 0];
    }
}

fn encode_relative_jump(to_addr: usize, from_addr: usize) -> Result<u32, ExceptionError> {
    // COND 101 L signed_immed_24
    // AL=1110
    // to_addr = (from_addr + 8 + (signed_immed_24 << 2))
    // to_addr - from_addr = 8 + (signed_immed_24 << 2)
    // to_addr - from_addr - 8 = signed_immed_24 << 2
    // (to_addr - from_addr - 8) >> 2 = signed_immed_24

    if (to_addr & 3) != 0 || (from_addr & 3) != 0 {
        return Err(ExceptionError::JumpTargetAlignment);
    }

    let immed_24_unchecked = ((to_addr - from_addr - 8) as isize >> 2) as usize;
    let expressible = {
        let sign_ext = immed_24_unchecked & 0xff00_0000;
        sign_ext == 0 || sign_ext == 0xff00_0000
    };
    if !expressible {
        return Err(ExceptionError::TooFar {
            from: from_addr as usize,
            to: to_addr as usize,
        });
    }
    let immed_24 = immed_24_unchecked & 0x00ff_ffff;
    let instruction
        = 0xe000_0000 // COND=AL
        | 0x0a00_0000 // 101_
        | 0x0000_0000 // L=0
        | (immed_24 as u32) // signed_immed_24
        ;

    Ok(instruction)
}

#[macro_export]
macro_rules! def_irq_landing_pad {
    ($name:ident, $target:ident) => {
        unsafe extern "C" {
            pub static $name: [u32; 0];
        }
        global_asm!(
            ".globl {EXPORT_SYM}",
            "{EXPORT_SYM}:",
            "   sub lr, lr, #4",
            "   stmfd sp!,{{r0-r12, lr}}",
            "   mrs r0, spsr",
            "   stmfd sp!,{{r0}}",
            "   bl {TARGET_SYM}",
            "   ldmia sp!,{{r0}}",
            "   msr spsr, r0",
            "   ldmia sp!,{{r0-r12, pc}}^",
            EXPORT_SYM = sym $name,
            TARGET_SYM = sym $target,
        );
    };
}
