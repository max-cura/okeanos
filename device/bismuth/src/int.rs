use core::arch::asm;
use quartz::arch::arm1176::__dsb;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum InterruptMode {
    Neither = 0,
    FiqOnly = 1,
    IrqOnly = 2,
    Both = 3,
}

const IRQ_BIT: u32 = 0x0000_0080;
const FIQ_BIT: u32 = 0x0000_0040;

pub fn set_enabled_interrupts(mode: InterruptMode) -> InterruptMode {
    __dsb();
    let (clear_mask, set_mask) = match mode {
        InterruptMode::Neither => (0, IRQ_BIT | FIQ_BIT),
        InterruptMode::IrqOnly => (IRQ_BIT, FIQ_BIT),
        InterruptMode::FiqOnly => (FIQ_BIT, IRQ_BIT),
        InterruptMode::Both => (IRQ_BIT | FIQ_BIT, 0),
    };
    let mut cpsr_copy: u32;
    unsafe {
        asm!(
            "mrs {t0}, cpsr",
            "mov {cpsr_out}, {t0}",
            "and {t0}, {t0}, {clear_mask}",
            "orr {t0}, {t0}, {set_mask}",
            "msr cpsr, {t0}",
            t0 = out(reg) _,
            set_mask = in(reg) set_mask,
            clear_mask = in(reg) !clear_mask,
            cpsr_out = out(reg) cpsr_copy,
        );
    }
    __dsb();
    interrupt_mode_from_bits(cpsr_copy)
}

fn interrupt_mode_from_bits(cpsr: u32) -> InterruptMode {
    let (irq_enabled, fiq_enabled) = ((cpsr & IRQ_BIT) == 0, (cpsr & FIQ_BIT) == 0);
    match (irq_enabled, fiq_enabled) {
        (false, false) => InterruptMode::Neither,
        (true, false) => InterruptMode::IrqOnly,
        (false, true) => InterruptMode::FiqOnly,
        (true, true) => InterruptMode::Both,
    }
}

pub fn get_enabled_interrupts() -> InterruptMode {
    let mut out: u32;
    unsafe {
        asm!(
            "mrs {t0}, cpsr",
            t0 = out(reg) out,
        );
    }
    interrupt_mode_from_bits(out)
}

// TODO: Secure Monitor mode
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum OperatingMode {
    User,
    FIQ,
    IRQ,
    Supervisor,
    Abort,
    Undefined,
    System,
}
impl OperatingMode {
    pub fn to_bits(self) -> u32 {
        match self {
            OperatingMode::User => 0b10000,
            OperatingMode::FIQ => 0b10001,
            OperatingMode::IRQ => 0b10010,
            OperatingMode::Supervisor => 0b10011,
            OperatingMode::Abort => 0b10111,
            OperatingMode::Undefined => 0b11011,
            OperatingMode::System => 0b11111,
        }
    }
}

pub unsafe fn init_stack_for_mode(mode: OperatingMode, stack: usize) {
    // TODO: check stack alignment requirement
    assert_eq!(stack & 7, 0, "stack must be 8-byte aligned");
    match mode {
        OperatingMode::FIQ => unsafe {
            asm!(
                "mrs {t0}, cpsr",
                "cpsid f, #0b10001",
                "mov sp, {t2}",
                "msr cpsr, {t0}",
                t0 = out(reg) _,
                t2 = in(reg) stack,
            );
        },
        OperatingMode::IRQ => unsafe {
            asm!(
                "mrs {t0}, cpsr",
                "cpsid i, #0b10010",
                "mov sp, {t2}",
                "msr cpsr, {t0}",
                t0 = out(reg) _,
                t2 = in(reg) stack,
            );
        },
        OperatingMode::Supervisor => {
            // TODO: we currently assume that supervisor stack is already set up
            return;
        }
        OperatingMode::Abort => unsafe {
            asm!(
                "mrs {t0}, cpsr",
                "cpsid a, #0b10111",
                "mov sp, {t2}",
                "msr cpsr, {t0}",
                t0 = out(reg) _,
                t2 = in(reg) stack,
            );
        },
        OperatingMode::Undefined => unsafe {
            asm!(
                "mrs {t0}, cpsr",
                "cps #0b11011",
                "mov sp, {t2}",
                "msr cpsr, {t0}",
                t0 = out(reg) _,
                t2 = in(reg) stack,
            );
        },
        OperatingMode::User | OperatingMode::System => unsafe {
            asm!(
                "mrs {t0}, cpsr",
                "cps #0b11111",
                "mov sp, {t2}",
                "msr cpsr, {t0}",
                t0 = out(reg) _,
                t2 = in(reg) stack,
            );
        },
    }
}
