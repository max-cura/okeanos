use core::hint::unreachable_unchecked;
use core::arch::asm;

proc_bitfield::bitfield! {
    #[derive(Copy, Clone, Eq, PartialEq)]
    pub struct CPSR(u32): Debug, FromRaw, IntoRaw, DerefRaw {
        pub execution_state : u8  [unsafe_get! ExecutionState, set ExecutionState] @ 0..=4,
        pub thumb : bool @ 5,
        pub disable_fiq : bool @ 6,
        pub disable_irq : bool @ 7,
        pub disable_ida : bool @ 8,
        pub big_endian : bool [read_only] @ 9,
        // 10..=15 reserved
        pub ge_bits : u8 @ 16..=19,
        // 16..=23 reserved
        pub jazelle : bool @ 24,
        // 25..=26 reserved
        pub cond_q : bool @ 27,
        pub cond_v : bool @ 28,
        pub cond_c : bool @ 29,
        pub cond_z : bool @ 30,
        pub cond_n : bool @ 31,
    }
}

/// Error performing conversion from `u8` to [`ExecutionState`].
#[derive(Debug, Copy, Clone)]
pub struct ExecutionStateError(u8);
impl ExecutionStateError {
    pub fn value(&self) -> u8 { self.0 }
}

/// arm1176 execution state
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ExecutionState {
    User = 0b10000,
    FIQ = 0b10001,
    IRQ = 0b10010,
    Supervisor = 0b10011,
    Abort = 0b10111,
    Undefined = 0b11011,
    System = 0b11111,
}
impl Into<u8> for ExecutionState {
    fn into(self) -> u8 { self as u8 }
}
impl TryFrom<u8> for ExecutionState {
    type Error = ExecutionStateError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b10000 => Ok(Self::User),
            0b10001 => Ok(Self::FIQ),
            0b10010 => Ok(Self::IRQ),
            0b10011 => Ok(Self::Supervisor),
            0b10111 => Ok(Self::Abort),
            0b11011 => Ok(Self::Undefined),
            0b11111 => Ok(Self::System),
            _ => Err(ExecutionStateError(value))
        }
    }
}
impl proc_bitfield::UnsafeFrom<u8> for ExecutionState {
    unsafe fn unsafe_from(value: u8) -> Self {
        match value {
            0b10000 => Self::User,
            0b10001 => Self::FIQ,
            0b10010 => Self::IRQ,
            0b10011 => Self::Supervisor,
            0b10111 => Self::Abort,
            0b11011 => Self::Undefined,
            0b11111 => Self::System,
            _ => unreachable_unchecked(),
        }
    }
}

#[inline]
pub fn __read_cpsr() -> CPSR {
    let mut out : u32;
    unsafe {
        asm!(
        "mrs {tmp}, cpsr",
        tmp = out(reg) out
        )
    }
    CPSR(out)
}

#[inline]
pub fn __write_cpsr(cpsr: CPSR) {
    let inp : u32 = cpsr.into();
    unsafe {
        asm!(
        "msr cpsr, {tmp}",
        tmp = in(reg) inp
        )
    }
}
