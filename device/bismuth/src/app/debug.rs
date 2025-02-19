use crate::exceptions::VectorBuilder;
use crate::int::OperatingMode;
use crate::{int, steal_println};
use alloc::boxed::Box;
use bcm2835_lpa::Peripherals;
use core::alloc::Layout;
use core::arch::{asm, global_asm};
use proc_bitfield::Bitfield;
use quartz::arch::arm1176::__dsb;
use quartz::cpreg;
use quartz::device::bcm2835::timing::delay_millis;

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct DSCR(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// 1 if rDTR full
        pub rdtrfull: bool [ro] @ 30,
        /// 1 if wDTR full
        pub wdtrfull: bool [ro] @ 29,
        /// Imprecise data aborts ignored
        pub imprecise_data_aborts_ignored: bool [ro] @ 19,
        /// 1 if processor is in Non-secure state
        pub nswstatus: bool [ro] @ 18,
        pub spniden: bool [ro] @ 17,
        pub spiden: bool [ro] @ 16,
        /// 1=enabled; in order for core to take exception, must be both selected and enabled
        pub monitor_debug_enable : bool @ 15,
        /// 0=monitor debug-mode, 1=halting debug-mode
        pub mode_select: bool [ro] @ 14,
        pub execute_arm_instr_enable: bool [ro] @ 13,
        /// 0=user mode access to comms channel enabled
        /// basically, whether to allow user mode access to cp14 debug register
        pub user_mode_comms: bool @ 12,
        /// 1=interrupts disabled
        /// if bit is set, IRQ and FIQ input signals are inhibited
        pub inhibit_interrupts: bool [ro] @ 11,
        pub dbg_ack : bool [ro] @ 10,
        pub dbgnopwrdwn: bool [ro] @ 9,
        pub sticky_undefined: bool [ro] @ 8,
        pub sticky_imprecise_data_aborts: bool [ro] @ 7,
        pub sticky_precise_data_aborts: bool [ro] @ 6,
        pub debug_entry: u8 @ 2..=5,
        pub core_restarted: bool [ro] @ 1,
        pub core_halted: bool [ro] @ 1
    }
}

#[repr(u8)]
pub enum DebugEntryMethod {
    HaltDBGTAP = 0b0000,
    Breakpoint = 0b0001,
    Watchpoint = 0b0010,
    BKPTInstr = 0b0011,
    EDBGRQSignal = 0b0100,
    VectorCatch = 0b0101,
}
impl TryFrom<u8> for DebugEntryMethod {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b0000 => Ok(DebugEntryMethod::HaltDBGTAP),
            0b0001 => Ok(DebugEntryMethod::Breakpoint),
            0b0010 => Ok(DebugEntryMethod::Watchpoint),
            0b0011 => Ok(DebugEntryMethod::BKPTInstr),
            0b0100 => Ok(DebugEntryMethod::EDBGRQSignal),
            0b0101 => Ok(DebugEntryMethod::VectorCatch),
            x => Err(x),
        }
    }
}

proc_bitfield::bitfield! {
    /// Virtual address of the instruction that caused the watchpoint
    /// Has the address of the instruction causing the watchpoint plus 8
    /// Unaffected when a precise Data Abort of Prefetch Abort occurs
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct WFAR(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        pub bits: u32 @ 0..=31
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct BVRWithContextId(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        address: u32 @ 2..=31,
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct BCRWithContextId(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        pub meaning: u8 @ 21..=22,
        /// when this bit is high, the corresponding BRP
        pub enable_linking: bool @ 20,
        pub linked_brp_number: u8 @ 16..=19,
        /// 00=S or NS, 01=NS, 10=S
        pub world: u8 @ 14..=15,
        /// 0000=breakpoint never matches
        /// xxx1=byte at `BVR[31:2],00` is accessed -> match
        /// xx1x=byte at `BVR[31:2],01` is accessed -> match
        /// x1xx=byte at `BVR[31:2],10` is accessed -> match
        /// 1xxx=byte at `BVR[31:2],11` is accessed -> match
        /// If BRP is programmed for context ID comparison, then this field must be 1111.
        pub byte_address_select: u8 @ 5..=8,
        /// 01=privileged, 10=user, 11=either
        /// not actually sure what this does
        pub supervisor_access: u8 @ 1..=2,
        /// 1=enabled 0=disabled
        pub enable: bool @ 0,
    }
}

#[repr(u8)]
pub enum BVRMeaning {
    IMVAMatch = 0b00,
    ContextIdMatch = 0b01,
    IMVAMismatch = 0b10,
}
impl TryFrom<u8> for BVRMeaning {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(BVRMeaning::IMVAMatch),
            0b01 => Ok(BVRMeaning::ContextIdMatch),
            0b10 => Ok(BVRMeaning::IMVAMismatch),
            x => Err(x),
        }
    }
}

// WVR is just u32[31:2], [1:0]=00

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct WCR(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// 0=disabled 1=enabled
        pub enable_linking: bool @ 20,
        pub linked_brp: u8 @ 16..=19,
        /// 00=either 01=ns 10=s
        pub world: u8 @ 14..=15,
        /// as normal
        pub byte_address_select: u8 @ 5..=8,
        pub on_store: bool @ 4,
        pub on_load: bool @ 3,
        /// 01=priv 10=user 11=either
        pub supervisor_access: u8 @ 1..=2,
        pub enable: bool @ 0,
    }
}

cpreg!(dscr, p14, 0, c0, c1, 0);
cpreg!(wfar, p14, 0, c0, c6, 0);
cpreg!(dsccr, p14, 0, c0, c10, 0);
cpreg!(dsmcr, p14, 0, c0, c11, 0);

cpreg!(bvr0, p14, 0, c0, c0, 4);
cpreg!(bvr1, p14, 0, c0, c1, 4);
cpreg!(bvr2, p14, 0, c0, c2, 4);
cpreg!(bvr3, p14, 0, c0, c3, 4);
cpreg!(bvr4, p14, 0, c0, c4, 4); // context id available
cpreg!(bvr5, p14, 0, c0, c5, 4); // context id available

cpreg!(bcr0, p14, 0, c0, c0, 5);
cpreg!(bcr1, p14, 0, c0, c1, 5);
cpreg!(bcr2, p14, 0, c0, c2, 5);
cpreg!(bcr3, p14, 0, c0, c3, 5);
cpreg!(bcr4, p14, 0, c0, c4, 5);
cpreg!(bcr5, p14, 0, c0, c5, 5);

cpreg!(wvr0, p14, 0, c0, c0, 6);
cpreg!(wvr1, p14, 0, c0, c1, 6);

cpreg!(wcr0, p14, 0, c0, c0, 7);
cpreg!(wcr1, p14, 0, c0, c1, 7);

cpreg!(dfsr, p15, 0, c5, c0, 0); // data fault status
cpreg!(ifsr, p15, 0, c5, c0, 1); // instruction fault status
cpreg!(far, p15, 0, c6, c0, 0);
//cpreg!(wfar, p15, 0, c6, c0, 1); // deprecated
cpreg!(ifar, p15, 0, c6, c0, 2);

unsafe extern "C" {
    static _ex_pabt: [u32; 0];
    static _ex_dabt: [u32; 0];
}
global_asm!(
    ".globl {EXPORT_SYM}",
    ".extern {TARGET_SYM}",
    "{EXPORT_SYM}:",
    // "   sub sp, sp, #8",
    // "   srsia {MODE_ABORT}",
    "   push {{r0-r12, lr}}",
    "   sub r0, lr, #4",
    "   bl {TARGET_SYM}",
    "   pop {{r0-r12, lr}}",
    // "   rfeia sp!",
    "   subs pc, lr, #4",
    EXPORT_SYM = sym _ex_pabt,
    TARGET_SYM = sym ex_pabt,
    // MODE_ABORT = const 0b10111,
);
global_asm!(
    ".globl {EXPORT_SYM}",
    ".extern {TARGET_SYM}",
    "{EXPORT_SYM}:",
    // "   sub sp, sp, #8",
    // "   srsia {MODE_ABORT}",
    "   push {{r0-r12, lr}}",
    "   bl {TARGET_SYM}",
    "   pop {{r0-r12, lr}}",
    "   subs pc, lr, #4",
    EXPORT_SYM = sym _ex_dabt,
    TARGET_SYM = sym ex_dabt,
    // MODE_ABORT = const 0b10111,
);

extern "C" fn ex_pabt(ret_addr: usize) {
    let dscr = unsafe { dscr::read() };
    let ifsr = unsafe { ifsr::read() };
    let far = unsafe { far::read() };
    let ifar = unsafe { ifar::read() };

    unsafe {
        bvr0::write(ret_addr);
    }
    let abort_at = braked_up as usize;
    if abort_at == ret_addr {
        steal_println!("stopping slide");
        unsafe {
            bcr0::write(0);
        }
    }

    match DebugEntryMethod::try_from(DSCR(dscr as u32).debug_entry()) {
        Ok(DebugEntryMethod::BKPTInstr) => {
            steal_println!(
                "PREFETCH ABORT (bkpt) : dscr={dscr:08x} ifsr={ifsr:08x} from={ret_addr:08x} far={far:08x} ifar={ifar:08x}"
            );
        }
        Ok(x) => {
            steal_println!(
                "PREFETCH ABORT : dscr={dscr:08x} ifsr={ifsr:08x} from={ret_addr:08x} far={far:08x} ifar={ifar:08x}"
            );
        }
        Err(e) => {
            steal_println!("unknown DSCR entry value: {e}");
        }
    }
}
extern "C" fn ex_dabt() {
    let dfsr = unsafe { dfsr::read() };
    let far = unsafe { far::read() };
    let wfar = unsafe { wfar::read() };
    steal_println!("DATA ABORT : dfsr={dfsr:08x} far={far:08x} wfar={wfar:08x}");
}

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn braked_up() {
    steal_println!("braked up");
}

pub fn run() {
    unsafe {
        quartz::arch::arm1176::mmu::__disable_mmu();
    }

    steal_println!("allocating vector destination");

    let layout = Layout::array::<u32>(8).unwrap();
    let vdi_ptr: *mut u32 = core::ptr::with_exposed_provenance_mut(0);
    let vdi_len = layout.size();
    let vector_dst = core::ptr::slice_from_raw_parts_mut(vdi_ptr, vdi_len);

    steal_println!("vector_dst={vector_dst:08x?}");

    steal_println!("initialzing ABORT stack");
    unsafe { crate::int::init_stack_for_mode(OperatingMode::Abort, 0x0800_0000) };

    __dsb();

    let mut b: *mut u32 = Box::into_raw(Box::new(0u32));
    let b_addr = b.addr();
    steal_println!("allocated box at {b_addr:08x}");

    __dsb();

    unsafe {
        dscr::write(
            DSCR(dscr::read() as u32)
                .with_monitor_debug_enable(true)
                .into_storage() as usize,
        );
    }
    unsafe {
        steal_println!("enabled debug monitor mode");
        wcr0::write(WCR(wcr0::read() as u32).with_enable(false).into_storage() as usize);
        wvr0::write(b_addr);
        wcr0::write(
            WCR(0)
                .with_enable_linking(false)
                .with_byte_address_select(0b0011)
                .with_supervisor_access(3)
                .with_world(0)
                .with_on_load(true)
                .with_on_store(true)
                .with_enable(true)
                .into_storage() as usize,
        );
        steal_println!("set WRP0");

        wcr1::write(WCR(0).into_storage() as usize);
        steal_println!("disabled WRP1");
    }

    __dsb();

    steal_println!("installing vector table");
    let vector = VectorBuilder::new()
        .set_prefetch_abort_handler(unsafe { &raw const _ex_pabt })
        .set_data_abort_handler(unsafe { &raw const _ex_dabt });
    unsafe { vector.install(vector_dst) }.unwrap();

    __dsb();

    steal_println!("b={b:08x?}");

    unsafe {
        // asm!("bkpt");
        b.write_volatile(12);
    }
    let c = unsafe { b.read_volatile() };
    __dsb();

    let abort_at = braked_up as usize;
    steal_println!("c={:08x}", c);
    steal_println!("braked up @ {abort_at:08x}");

    __dsb();

    unsafe {
        bcr0::write(
            BCRWithContextId(bcr0::read() as u32)
                .with_enable(false)
                .into_storage() as usize,
        );
        bvr0::write(abort_at);
        bcr0::write(
            BCRWithContextId(0)
                .with_meaning(BVRMeaning::IMVAMatch as u8)
                .with_enable_linking(false)
                .with_world(0)
                .with_supervisor_access(3)
                .with_byte_address_select(0b1111)
                .with_enable(true)
                .into_storage() as usize,
        );
    }

    __dsb();

    unsafe {
        int::init_stack_for_mode(OperatingMode::User, 0x0f00_0000);
        asm!("cps #0b10000");
    }

    __dsb();

    #[unsafe(no_mangle)]
    #[inline(never)]
    fn fib(a: u32, b: u32, n: usize) -> u32 {
        if n > 0 { fib(b, a + b, n - 1) } else { a + b }
    }
    let f = fib(1, 1, 3);

    __dsb();

    // steal_println!("before brake");

    braked_up();

    steal_println!("done, f = {f}");

    // loop {}

    unsafe {
        bcr0::write(0);
        dscr::write(
            DSCR(dscr::read() as u32)
                .with_monitor_debug_enable(false)
                .into_storage() as usize,
        );
    }
    let _ = b;
}
