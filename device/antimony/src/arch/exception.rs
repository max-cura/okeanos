//! Manages the trap vector.

use crate::arch::{Mtvec, mcause, mepc, mtval, mtvec};
use crate::println;
use alloc::vec::Vec;
use proc_bitfield::{bits, set_bits, with_bits};
use thiserror::Error;

#[macro_export]
macro_rules! define_exception_trampoline {
($v:vis $name:ident -> $target:ident) => {
#[naked]
$v extern "C" fn $name() -> ! {
    unsafe {
        ::core::arch::naked_asm!(
            r#"
            // we just use whichever stack happens to be in place already

            // save registers: ra, t0-t6, a0-a7, which are x1, x6-7, x10-17, x28-31
            // sp we're expected to fix ourselves, gp we don't touch, tp we don't touch unless we
            // do, and s0-s11 are callee-saved so whoever we call will take care of those if needed!

            addi sp, sp, -16 * 8

            sd ra, 0 * 8(sp)
            sd t0, 1 * 8(sp); sd t1, 2 * 8(sp); sd t2, 3 * 8(sp); sd t3, 4 * 8(sp); sd t4, 5 * 8(sp)
            sd t5, 6 * 8(sp); sd t6, 7 * 8(sp)
            sd a0, 8 * 8(sp); sd a1, 9 * 8(sp); sd a2, 10 * 8(sp); sd a3, 11 * 8(sp)
            sd a4, 12 * 8(sp); sd a5, 13 * 8(sp); sd a6, 14 * 8(sp); sd a7, 15 * 8(sp)

            mv a0, sp
            jal ra, {target}

            ld ra, 0 * 8(sp)
            ld t0, 1 * 8(sp); ld t1, 2 * 8(sp); ld t2, 3 * 8(sp); ld t3, 4 * 8(sp); ld t4, 5 * 8(sp)
            ld t5, 6 * 8(sp); ld t6, 7 * 8(sp)
            ld a0, 8 * 8(sp); ld a1, 9 * 8(sp); ld a2, 10 * 8(sp); ld a3, 11 * 8(sp)
            ld a4, 12 * 8(sp); ld a5, 13 * 8(sp); ld a6, 14 * 8(sp); ld a7, 15 * 8(sp)

            addi sp, sp, 16 * 8

            mret
            "#,
            target = sym $target
        )
    }
}
}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Target {
    Default,
    Jump(usize),
}
impl Target {
    fn to_jump_addr(self) -> usize {
        match self {
            Target::Default => (default_trap_handler as *const ()).addr(),
            Target::Jump(ja) => ja,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct TrapFrame {
    pub ra: usize,
    pub t: [usize; 7],
    pub a: [usize; 8],
}

pub extern "C" fn default_trap_handler(trap_frame: *mut TrapFrame) {
    let cause = mcause::read();
    let epc = mepc::read();
    let tval = mtval::read();
    println!("trap C{cause:?} E{epc:x} T{tval:x} frame={:x?}", unsafe {
        trap_frame.read_volatile()
    });
    loop {}
    // unsafe {
    //     // clear M-mode software interrupts
    //     // NOTE: not clear from the documentation, but this is a "clear bit by setting 1" register
    //     d1_pac::Peripherals::steal()
    //         .CLINT
    //         .msip()
    //         .write(|w| w.bits(1))
    // }
}
define_exception_trampoline!(_default_trampoline -> default_trap_handler);

pub const MCAUSE_INTERRUPT_FLAG: u64 = 1 << 63;

/// Binary-compatible with the [`mcause`] register.
#[repr(u64)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Trap {
    SupervisorSoftwareInterrupt = 0x1 | MCAUSE_INTERRUPT_FLAG,
    MachineSoftwareInterrupt = 0x3 | MCAUSE_INTERRUPT_FLAG,
    SupervisorTimerInterrupt = 0x5 | MCAUSE_INTERRUPT_FLAG,
    MachineTimerInterrupt = 0x7 | MCAUSE_INTERRUPT_FLAG,
    SupervisorExternalInterrupt = 0x9 | MCAUSE_INTERRUPT_FLAG,
    MachineExternalInterrupt = 0xb | MCAUSE_INTERRUPT_FLAG,
    PMUOverflowInterrupt = 0x11 | MCAUSE_INTERRUPT_FLAG,

    FetchInstructionAccessException = 0x1,
    IllegalInstructionException = 0x2,
    DebugBreakpointException = 0x3,
    LoadInstructionUnalignedException = 0x4,
    LoadInstructionAccessException = 0x5,
    StoreOrAtomicUnalignedException = 0x6,
    StoreOrAtomicAccessException = 0x7,
    UserEnvCallException = 0x8,
    SupervisorEnvCallException = 0x9,
    MachineEnvCallException = 0xb,
    FetchInstructionPageException = 0xc,
    LoadInstructionPageException = 0xd,
    StoreOrAtomicPageException = 0xf,
}
impl Trap {
    pub fn is_interrupt(self) -> bool {
        bits!(self as u64, 63)
    }
    pub fn vector_offset(self) -> usize {
        with_bits!(self as u64, 63 = false) as usize
    }
}

pub struct VectorBuilder {
    /// async interrupt with cause `c` goes to BASE+4*i
    /// sync exceptions go to BASE always
    vector: [Target; 256],
}
impl VectorBuilder {
    pub fn new() -> Self {
        Self {
            vector: [Target::Default; 256],
        }
    }
    pub unsafe fn set_unchecked(mut self, i: usize, target: Target) -> Self {
        self.vector[i] = target;
        self
    }
    pub fn set(mut self, trap: Trap, target: Target) -> Self {
        if !trap.is_interrupt() {
            panic!("VectorBuilder cannot set vectored trap for {trap:?}: not an interrupt");
        }
        self.vector[trap.vector_offset()] = target;
        self
    }
    pub fn install(self, dest: *mut [u32; 256]) -> Result<(), InstallError> {
        if !dest.is_aligned_to(16) {
            return Err(InstallError::DestinationAlignment);
        }
        let base = dest.addr();
        #[rustfmt::skip]
        let indices : [usize; 256] = (0usize..256).collect::<Vec<_>>().try_into().unwrap();
        let sources = indices.map(|i| base + (i * 4));
        let encodings = indices.try_map(|i| encode(self.vector[i].to_jump_addr(), sources[i]))?;

        // disable interrupts
        // write new table
        // full memory + IO fence (IO because of possible issues with peripheral interrupts?)
        // FENCE.I to ensure that icache is updated
        critical_section::with(|_| unsafe {
            dest.write_volatile(encodings);

            super::mem::fence!(iorw, iorw);
            super::mem::fence_i();

            mtvec::write(Mtvec(0).with_base((base >> 2) as u64).with_mode(1));

            println!("mtvec={:x}", mtvec::read().0);
        });

        Ok(())
    }
}

fn encode(to: usize, from: usize) -> Result<u32, InstallError> {
    // jal x0, imm20
    // pc <- pc + sext(imm20 << 1)
    // imm[20], imm10:1], imm[11], imm[19:12], rd, 110 1111
    let imm20s1 = (to - from) as isize;
    let truncated = imm20s1 >> 21;
    if !(truncated == 0 || !truncated == 0) {
        return Err(InstallError::TargetTooFar(to, from));
    }
    if bits!(imm20s1, 0) {
        return Err(InstallError::JumpOddBytes);
    }
    let imm20: u32 = (imm20s1 >> 1) as u32;
    let mut target: u32 = 0x0000_0000;

    set_bits!(target, 0..=6 = 0b110_1111);
    set_bits!(target, 7..=11 = 0);
    let imm11_18: usize = bits!(imm20, 11..=18);
    set_bits!(target, 12..=19 = imm11_18);
    set_bits!(target, 20 = bits!(imm20, 10));
    let imm0_9: usize = bits!(imm20, 0..=9);
    set_bits!(target, 21..=30 = imm0_9);
    set_bits!(target, 31 = bits!(imm20, 20));

    Ok(target)
}

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("relative jump is odd number of bytes")]
    JumpOddBytes,
    #[error("jump target ({0:x}) too far from jump source ({1:x})")]
    TargetTooFar(usize, usize),
    #[error("installation destination pointer misaligned")]
    DestinationAlignment,
}
