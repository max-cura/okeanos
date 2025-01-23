use crate::{UnsafeSync, uart1_println};
use bcm2835_lpa::Peripherals;
use core::arch::asm;
use core::cell::UnsafeCell;
use quartz::arch::arm1176::__dsb;
use quartz::device::bcm2835::timing::__floating_time;
use thiserror::Error;
// TODO: special FIQ handling (direct installation)

unsafe extern "C" {
    static _landing_pad_svc: [u32; 0];
    static _landing_pad_smc: [u32; 0];
    static _landing_pad_undef: [u32; 0];
    static _landing_pad_pabt: [u32; 0];
    static _landing_pad_fiq: [u32; 0];
    static _landing_pad_irq: [u32; 0];
    static _landing_pad_dabt: [u32; 0];
    static _landing_pad_reset: [u32; 0];
    static _landing_pad_bkpt: [u32; 0];
    static _landing_pad_none: [u32; 0];
}

#[unsafe(no_mangle)]
pub extern "C" fn _interrupt_svc(r0: u32, r1: u32, r2: u32, swi_immed: u32, r3: u32) {
    let peri = unsafe { Peripherals::steal() };
    uart1_println!(
        &peri.UART1,
        "_interrupt_svc({swi_immed:08x}) r0={r0:08x} r1={r1:08x} r2={r2:08x} r3={r3:08x}"
    );
}
pub static X: UnsafeSync<UnsafeCell<(u32, u32, u32)>> = UnsafeSync(UnsafeCell::new((0, 0, 0)));
#[unsafe(no_mangle)]
pub extern "C" fn _interrupt_irq() {
    let peri = unsafe { Peripherals::steal() };
    // uart1_println!(&peri.UART1, "irq");
    __dsb();
    let timer_pending = unsafe { peri.LIC.basic_pending().read().timer().bit_is_set() };
    __dsb();
    if timer_pending {
        let tim_val = 0x2000b404 as *mut u32;
        let tim_irq_clr = 0x2000b40c as *mut u32;
        __dsb();
        unsafe {
            tim_irq_clr.write_volatile(1);
        }
        let tv = unsafe { tim_val.read_volatile() };
        __dsb();
        uart1_println!(
            &peri.UART1,
            "irq: timer_pending at {} / {}",
            quartz::device::bcm2835::timing::__floating_time(&peri.SYSTMR),
            tv
        );
        let now = __floating_time(&peri.SYSTMR);
        __dsb();
        let (c, s, lc) = unsafe { X.0.get().read_volatile() };
        let p = if lc != 0 { now as u32 - lc } else { 0 };
        unsafe {
            X.0.get().write_volatile((c + 1, s + p, now as u32));
        }
        __dsb();
    }
}

#[derive(Debug, Error, Copy, Clone)]
pub enum InterruptError {
    #[error("can't produce jump instruction from {from:08x} to {to:08x}")]
    TooFar { from: usize, to: usize },
    #[error("jump table address must be 32-byte aligned")]
    TableAlignment,
    #[error("to- and from- addresses must be 4-byte aligned")]
    InstructionAlignment,
}

pub unsafe fn install_interrupts(ptr: *mut [u32; 8]) -> Result<(), InterruptError> {
    let save = get_enabled_interrupts();
    set_enabled_interrupts(InterruptMode::Neither);
    let result
        // SAFETY: will never overwrite `ptr` with badly encoded instructions
        = install_jumptable_at(ptr)
        // SAFETY: if install_jumptable_at() succeeds, then set_vector_base_address_register() also
        //         will, because set_vector_base_address_register() will only fail with
        //         TableAlignment, which is already checked by install_jumptable_at
        .and_then(|()| unsafe { set_vector_base_address_register(ptr) });
    set_enabled_interrupts(save);
    result
}

fn encode_relative_jump(to_addr: usize, from_addr: usize) -> Result<u32, InterruptError> {
    // COND 101 L signed_immed_24
    // AL=1110
    // to_addr = (from_addr + 8 + (signed_immed_24 << 2))
    // to_addr - from_addr = 8 + (signed_immed_24 << 2)
    // to_addr - from_addr - 8 = signed_immed_24 << 2
    // (to_addr - from_addr - 8) >> 2 = signed_immed_24

    if (to_addr & 3) != 0 || (from_addr & 3) != 0 {
        return Err(InterruptError::InstructionAlignment);
    }

    let immed_24_unchecked = ((to_addr - from_addr - 8) as isize >> 2) as usize;
    let expressible = {
        let sign_ext = immed_24_unchecked & 0xff00_0000;
        sign_ext == 0 || sign_ext == 0xff00_0000
    };
    if !expressible {
        return Err(InterruptError::TooFar {
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

unsafe fn install_jumptable_at(ptr: *mut [u32; 8]) -> Result<(), InterruptError> {
    if !ptr.is_aligned_to(0x20) {
        return Err(InterruptError::TableAlignment);
    }
    let table_addr = ptr.expose_provenance();
    let jump_addresses = [0x0, 0x4, 0x8, 0xc, 0x10, 0x14, 0x18, 0x1c].map(|x| table_addr + x);
    let pad_addresses = [
        (&raw const _landing_pad_reset).addr(), // 0x00
        (&raw const _landing_pad_undef).addr(), // 0x04
        (&raw const _landing_pad_svc).addr(),   // 0x08
        (&raw const _landing_pad_pabt).addr(),  // 0x0c
        (&raw const _landing_pad_dabt).addr(),  // 0x10
        (&raw const _landing_pad_none).addr(),  // 0x14
        (&raw const _landing_pad_irq).addr(),   // 0x18
        (&raw const _landing_pad_fiq).addr(),   // 0x1c
    ];
    let encodings = [0, 1, 2, 3, 4, 5, 6, 7]
        .try_map(|i| encode_relative_jump(pad_addresses[i], jump_addresses[i]))?;

    unsafe {
        let peri = unsafe { Peripherals::steal() };
        uart1_println!(
            &peri.UART1,
            "installing encodings: {encodings:08x?} to {ptr:08x?}"
        );
        ptr.write_volatile(encodings);
    }

    Ok(())
}

unsafe fn set_vector_base_address_register(ptr: *mut [u32; 8]) -> Result<(), InterruptError> {
    if !ptr.is_aligned_to(0x20) {
        return Err(InterruptError::TableAlignment);
    }
    let addr = ptr.expose_provenance();
    unsafe {
        asm!(
            "mcr p15, 0, {t0}, c12, c0, 0",
            t0 = in(reg) addr
        );
    }
    Ok(())
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum InterruptMode {
    Neither = 0,
    FiqOnly = 1,
    IrqOnly = 2,
    Both = 3,
}

const IRQ_BIT: u32 = 0x0000_0080;
const FIQ_BIT: u32 = 0x0000_0040;

pub fn set_enabled_interrupts(mode: InterruptMode) {
    __dsb();
    let (clear_mask, set_mask) = match mode {
        InterruptMode::Neither => (0, IRQ_BIT | FIQ_BIT),
        InterruptMode::IrqOnly => (IRQ_BIT, FIQ_BIT),
        InterruptMode::FiqOnly => (FIQ_BIT, IRQ_BIT),
        InterruptMode::Both => (IRQ_BIT | FIQ_BIT, 0),
    };
    unsafe {
        asm!(
            "mrs {t0}, cpsr",
            "and {t0}, {t0}, {clear_mask}",
            "orr {t0}, {t0}, {set_mask}",
            "msr cpsr, {t0}",
            t0 = out(reg) _,
            set_mask = in(reg) set_mask,
            clear_mask = in(reg) !clear_mask,
        );
    }
    __dsb();
}

pub fn get_enabled_interrupts() -> InterruptMode {
    let mut out: u32;
    unsafe {
        asm!(
            "mrs {t0}, cpsr",
            t0 = out(reg) out,
        );
    }
    let (irq_enabled, fiq_enabled) = ((out & IRQ_BIT) == 0, (out & FIQ_BIT) == 0);
    match (irq_enabled, fiq_enabled) {
        (false, false) => InterruptMode::Neither,
        (true, false) => InterruptMode::IrqOnly,
        (false, true) => InterruptMode::FiqOnly,
        (true, true) => InterruptMode::Both,
    }
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
        OperatingMode::IRQ => {
            asm!(
                "mrs {t0}, cpsr",
                "cpsid i, #0b10010",
                "mov sp, {t2}",
                "msr cpsr, {t0}",
                t0 = out(reg) _,
                t2 = in(reg) stack,
            );
        }
        OperatingMode::Supervisor => {
            // TODO: we currently assume that supervisor stack is already set up
            return;
        }
        OperatingMode::Abort => {
            asm!(
                "mrs {t0}, cpsr",
                "cpsid a, #0b10111",
                "mov sp, {t2}",
                "msr cpsr, {t0}",
                t0 = out(reg) _,
                t2 = in(reg) stack,
            );
        }
        OperatingMode::Undefined => {
            asm!(
                "mrs {t0}, cpsr",
                "cps #0b11011",
                "mov sp, {t2}",
                "msr cpsr, {t0}",
                t0 = out(reg) _,
                t2 = in(reg) stack,
            );
        }
        OperatingMode::User | OperatingMode::System => {
            asm!(
                "mrs {t0}, cpsr",
                "cps #0b11111",
                "mov sp, {t2}",
                "msr cpsr, {t0}",
                t0 = out(reg) _,
                t2 = in(reg) stack,
            );
        }
    }
}
