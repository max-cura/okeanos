use quartz::arch::arm1176::{dmb, dsb, prefetch_flush};
use thiserror::Error;

quartz::define_coprocessor_registers! {
    vector_base_address => p15 0 c12 c0 0;
}

/// Define an `extern "C"` "function" with name `$name` that can be placed in the IRQ vector slot
/// and will call out to `$target`.
#[macro_export]
macro_rules! define_irq_trampoline {
    ($name:ident, $target:ident) => {
        unsafe extern "C" {
            pub static $name: [u32; 0];
        }
        ::core::arch::global_asm!(
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

/// `$target` expects: `fn(x) -> usize` where `x` is the address of the instruction that had the
/// prefetch abort.
#[macro_export]
macro_rules! define_pabt_trampoline {
    ($name:ident, $target:ident) => {
        unsafe extern "C" { pub static $name: [u32; 0]; }
        ::core::arch::global_asm!(
        r#"
            .globl {EXPORT_SYM}
            .extern {TARGET_SYM}
            {EXPORT_SYM}:
                push {{r0-r12}}
                sub r0, lr, #4
                mov r1, sp
                bl {TARGET_SYM}
                add lr, r0, #4
                pop {{r0-r12}}
                subs pc, lr, #4
        "#,
            EXPORT_SYM = sym $name,
            TARGET_SYM = sym $target,
        );
    }
}
/// `$target` expects: `fn(x) -> ()` where `x` is the address of the Load or Store instruction that
/// generated the data abort.
#[macro_export]
macro_rules! define_dabt_trampoline {
    ($name:ident, $target:ident) => {
        unsafe extern "C" { pub static $name: [u32; 0]; }
        ::core::arch::global_asm!(
        r#"
            .globl {EXPORT_SYM}
            .extern {TARGET_SYM}
            {EXPORT_SYM}:
                push {{r0-r12, lr}}
                sub r0, lr, #8
                bl {TARGET_SYM}
                pop {{r0-r12, lr}}
                subs pc, lr, #4
            "#,
            EXPORT_SYM = sym $name,
            TARGET_SYM = sym $target,
        );
    };
}

/*
This one looks a bit different; for a start, we can rely on register saving to work in our
favor here.

SAFETY: STACK NEEDS TO HAVE 2 WORDS OF SPACE AT THE TOP
*/
#[macro_export]
macro_rules! define_svc_trampoline {
    ($name:ident, $target:ident) => {
        unsafe extern "C" { pub static $name: [u32; 0]; }
        ::core::arch::global_asm!(
            r#"
            .globl {EXPORT_SYM}
            .extern {TARGET_SYM}
            {EXPORT_SYM}:
                srsia #{SUPERVISOR_MODE}
                push {{r0-r2}}
                push {{r3, sp, lr}}
                ldr r3, [lr, #-4]
                mvn lr, #(0xff << 24)
                and r3, r3, lr
                bl {TARGET_SYM}
                ldr r3, [sp, #0]
                add sp, sp, #12
                pop {{r0-r2}}
                rfeia sp
            "#,
            EXPORT_SYM = sym $name,
            TARGET_SYM = sym $target,
            SUPERVISOR_MODE = const 0b10011,
        );
    };
}

#[macro_export]
macro_rules! define_undef_trampoline {
    ($name:ident, $target:ident) => {
        unsafe extern "C" { pub static $name: [u32; 0]; }
        ::core::arch::global_asm!(
            r#"
            .globl {EXPORT_SYM}
            .extern {TARGET_SYM}
            {EXPORT_SYM}:
                srsia #{UNDEFINED_MODE}
                push {{r0-r12, lr}}
                sub r0, lr, #4
                bl {TARGET_SYM}
                pop {{r0-r12, lr}}
                rfeia sp
            "#,
            EXPORT_SYM = sym $name,
            TARGET_SYM = sym $target,
            UNDEFINED_MODE = const 0b11011,
        );
    };
}

#[macro_export]
macro_rules! define_reset_trampoline {
    ($name:ident, $target:ident) => {
        unsafe extern "C" { pub static $name: [u32; 0]; }
        ::core::arch::global_asm!(
            r#"
            .globl {EXPORT_SYM}
            .extern {TARGET_SYM}
            {EXPORT_SYM}:
                bl {TARGET_SYM}
            2:
                b 2b
            "#,
            EXPORT_SYM = sym $name,
            TARGET_SYM = sym $target,
        );
    };
}

#[macro_export]
macro_rules! define_fiq_trampoline {
    ($name:ident, $target:ident) => {
        unsafe extern "C" {
            pub static $name: [u32; 0];
        }
        ::core::arch::global_asm!(
            r#"
            .globl {EXPORT_SYM}
            .extern {TARGET_SYM}
            {EXPORT_SYM}:
                srsia #{FIQ_MODE}
                push {{r0-r12,lr}}
                bl {TARGET_SYM}
                pop {{r0-r12, lr}}
                rfeia sp
            "#,
            EXPORT_SYM = sym $name,
            TARGET_SYM = sym $target,
            FIQ_MODE = const 0b10001,
        );
    };
}

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

pub type CodePtr = usize;

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
        unsafe {
            Self {
                maps: [
                    defaults::_default_reset_trampoline.as_ptr().addr(),
                    defaults::_default_undef_trampoline.as_ptr().addr(),
                    defaults::_default_svc_trampoline.as_ptr().addr(),
                    defaults::_default_pabt_trampoline.as_ptr().addr(),
                    defaults::_default_dabt_trampoline.as_ptr().addr(),
                    defaults::_default_none_trampoline as usize,
                    defaults::_default_irq_trampoline.as_ptr().addr(),
                ],
                fiq: FiqAction::Jump(defaults::_default_fiq_trampoline.as_ptr().addr()),
            }
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
        let jump_target_addresses = self.maps;
        let jump_source_addresses =
            [0x0, 0x4, 0x8, 0xc, 0x10, 0x14, 0x18, 0x1c].map(|offset| dst_base_addr + offset);
        let encodings = [0, 1, 2, 3, 4, 5, 6].try_map(|i| {
            encode_relative_jump(jump_target_addresses[i], jump_source_addresses[i])
        })?;
        let fiq_buffer;
        let (fiq_ptr_src, fiq_words) = match self.fiq {
            FiqAction::Jump(fiq_jump_target) => {
                let fiq_jump_source = jump_source_addresses[7];
                fiq_buffer = encode_relative_jump(fiq_jump_target, fiq_jump_source)?;
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

        critical_section::with(|_| {
            dmb();
            for (idx, word) in source_iter.enumerate() {
                unsafe { dst_base.offset(idx as isize).write_volatile(word) };
            }
            dsb(); // ensure that table is written before CP15 access
            // NB: "any change involved with the processing of the exception itself (...) is
            // guaranteed to take effect" (ARM, B2-24).
            unsafe { vector_base_address::write_raw(dst_base_addr as u32) };
            prefetch_flush(); // flush pipeline to ensure base address is visible
        });

        Ok(())
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
            from: from_addr,
            to: to_addr,
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

mod defaults {
    use crate::steal_println;

    define_pabt_trampoline!(_default_pabt_trampoline, _landing_pad_pabt);
    define_dabt_trampoline!(_default_dabt_trampoline, _landing_pad_dabt);
    define_undef_trampoline!(_default_undef_trampoline, _landing_pad_undef);
    define_svc_trampoline!(_default_svc_trampoline, _landing_pad_svc);
    define_reset_trampoline!(_default_reset_trampoline, _landing_pad_reset);
    define_irq_trampoline!(_default_irq_trampoline, _landing_pad_irq);
    define_fiq_trampoline!(_default_fiq_trampoline, _landing_pad_fiq);

    pub extern "C" fn _landing_pad_svc(
        arg0: u32,
        arg1: u32,
        arg2: u32,
        imm: u32,
        arg3: u32,
        lr: u32,
    ) {
        steal_println!("swi {imm} {arg0:08x} {arg1:08x} {arg2:08x} {arg3:08x} lr={lr:08x}");
    }
    pub extern "C" fn _landing_pad_undef(addr: u32) {
        if addr < 0x2000_0000 {
            let value =
                unsafe { core::ptr::with_exposed_provenance::<u32>(addr as usize).read_volatile() };
            steal_println!("Encountered undefined instruction at {addr:08x}: {value:08x}");
        } else {
            steal_println!("Encountered undefined instruction at {addr:08x}");
        }
    }
    extern "C" fn _landing_pad_pabt(addr: u32) {
        steal_println!("Prefetch abort, addr={addr:08x}");
    }
    extern "C" fn _landing_pad_dabt(addr: u32) {
        steal_println!("Data abort, addr={addr:08x}");
    }
    extern "C" fn _landing_pad_irq() {
        steal_println!("IRQ interrupt");
    }
    extern "C" fn _landing_pad_fiq() {
        steal_println!("FIQ interrupt");
    }
    extern "C" fn _landing_pad_reset() {
        steal_println!("Reset vector called");
    }
    pub extern "C" fn _default_none_trampoline() {
        steal_println!("vector at exception_base+0x14 called");
        loop {}
    }
}
