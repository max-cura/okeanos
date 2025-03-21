use crate::exceptions::VectorBuilder;
use crate::int::OperatingMode;
use crate::{
    define_dabt_trampoline, define_pabt_trampoline, define_svc_trampoline, int, steal_println,
};
use alloc::boxed::Box;
use alloc::vec;
use core::alloc::Layout;
use core::any::Any;
use core::arch::global_asm;
use core::cell::SyncUnsafeCell;
use core::fmt::Debug;
use quartz::arch::arm1176::sync::ticket::TicketLock;
use quartz::arch::arm1176::{dmb, dsb, prefetch_flush};
use quartz::define_coprocessor_registers;

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct Dscr(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
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
    pub struct Wfar(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        pub bits: u32 @ 0..=31
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct Bvr(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        address: u32 @ 2..=31,
    }
}

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct Bcr(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
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
pub enum BvrMeaning {
    IMVAMatch = 0b00,
    ContextIdMatch = 0b01,
    IMVAMismatch = 0b10,
}
impl TryFrom<u8> for BvrMeaning {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(BvrMeaning::IMVAMatch),
            0b01 => Ok(BvrMeaning::ContextIdMatch),
            0b10 => Ok(BvrMeaning::IMVAMismatch),
            x => Err(x),
        }
    }
}

// WVR is just u32[31:2], [1:0]=00

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct Wcr(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
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

proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct Dfsr(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// 1=AXI slave 0=AXI decode
        decode_or_slave: bool @ 12,
        /// 1=write 0=read
        read_or_write: bool @ 11,
        status_bank : bool @ 10,
        domain : u8 @ 4..=7,
        status : u8 @ 0..=3,
    }
}
proc_bitfield::bitfield! {
    #[derive(Eq, PartialEq, Copy, Clone)]
    pub struct Ifsr(pub u32): Debug, IntoStorage, FromStorage, DerefStorage {
        /// 1=AXI slave, 0=AXI decode
        pub decode_or_slave: bool @ 12,
        pub status : u8 @ 0..=3,
    }
}

define_coprocessor_registers! {
    debug_status_control : Dscr => p14 0 c0 c1 0;
    watchpoint_fault_address : Wfar => p14 0 c0 c6 0;

    breakpoint_value_register_0 : Bvr => p14 0 c0 c0 4;
    // breakpoint_value_register_1 : BVR => p14 0 c0 c1 4;
    // breakpoint_value_register_2 : BVR => p14 0 c0 c2 4;
    // breakpoint_value_register_3 : BVR => p14 0 c0 c3 4;
    breakpoint_value_register_4 : Bvr => p14 0 c0 c4 4; /* context id */
    breakpoint_value_register_5 : Bvr => p14 0 c0 c5 4; /* context id */

    breakpoint_control_register_0 : Bcr => p14 0 c0 c0 5;
    // breakpoint_control_register_1 : BCR => p14 0 c0 c1 5;
    // breakpoint_control_register_2 : BCR => p14 0 c0 c2 5;
    // breakpoint_control_register_3 : BCR => p14 0 c0 c3 5;
    breakpoint_control_register_4 : Bcr => p14 0 c0 c4 5;
    breakpoint_control_register_5 : Bcr => p14 0 c0 c5 5;

    watchpoint_value_register_0 => p14 0 c0 c0 6;
    watchpoint_value_register_1 => p14 0 c0 c1 6;

    watchpoint_control_register_0: Wcr => p14 0 c0 c0 7;
    watchpoint_control_register_1: Wcr => p14 0 c0 c1 7;

    data_fault_status : Dfsr => p15 0 c5 c0 0;
    instruction_fault_status : Ifsr => p15 0 c5 c0 1;
    fault_address => p15 0 c6 c0 0;
    instruction_fault_address => p15 0 c6 c0 2;
}

const SYS_ELEVATE: u32 = 13;

define_pabt_trampoline!(_debug_prefetch_abort_trampoline, prefetch_abort);
define_dabt_trampoline!(_debug_data_abort_trampoline, data_abort);
define_svc_trampoline!(_debug_syscall_trampoline, syscall);

#[unsafe(no_mangle)]
pub extern "C" fn _interleave_stop() {
    unsafe {
        core::arch::asm!("nop");
    }
}
unsafe extern "C" {
    fn _interleave_wrap(test_fn: fn(), psr: usize);
}
global_asm!(
    r#"
    .globl _interleave_wrap
    .extern _interleave_stop
    _interleave_wrap:
        @ r0=test_fn r1=psr
        push {{r0-r3,lr}}
        blx r0
        bl _interleave_stop
        pop {{r0-r3,lr}}
        mov r2, #0
        mcr p15, 0, r2, c7, c10, 4 @ DSB
        mov r0, r1
        mov r1, lr
        swi 13
        mov r2, #0
        mcr p15, 0, r2, c7, c10, 4 @ DSB
        mov r0, #1
        bx r1
    "#
);
#[repr(C)]
#[derive(Debug)]
struct ThreadState {
    registers: [usize; 16],
    spsr: usize,
    did_stop: bool,
    hash: Option<crc32fast::Hasher>,
}
impl ThreadState {
    pub const fn empty() -> Self {
        Self {
            registers: [0; 16],
            spsr: 0,
            did_stop: false,
            hash: None,
        }
    }
}
trait Policy {
    fn next_thread(&mut self, last_thread: usize, threads: &[ThreadState]) -> Option<usize>;
    fn check(&mut self);
}
trait PolicyExt: Policy {
    type Result;
    fn result(&self) -> Self::Result;
    fn init(&self);
}
trait AnyPolicy: Policy {
    fn as_any(&self) -> &dyn Any;
}
impl<T: Policy + 'static> AnyPolicy for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}
struct InterleaveScheduler {
    policy: Option<Box<dyn AnyPolicy>>,
    current_thread: usize,

    threads: [ThreadState; 2],
}
impl Debug for InterleaveScheduler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InterleaveScheduler")
            .field("current_thread", &self.current_thread)
            .field("threads", &self.threads)
            .finish()
    }
}
unsafe impl Sync for InterleaveScheduler {}
impl InterleaveScheduler {
    pub const fn new() -> Self {
        Self {
            policy: None,
            current_thread: 0,
            threads: [ThreadState::empty(), ThreadState::empty()],
        }
    }
    pub fn run<P: PolicyExt + 'static>(&mut self, a: fn(), b: fn(), p: P) -> P::Result {
        self.current_thread = 0;
        self.threads = [ThreadState::empty(), ThreadState::empty()];
        for t in self.threads.iter_mut() {
            t.hash = Some(crc32fast::Hasher::new_with_initial(0));
        }
        p.init();
        unsafe {
            // set up breakpoint
            breakpoint_control_register_0::modify(|bcr0| bcr0.with_enable(false));
            breakpoint_value_register_0::write(Bvr(0));
            breakpoint_control_register_0::write(
                Bcr(0)
                    .with_meaning(BvrMeaning::IMVAMismatch as u8)
                    .with_enable_linking(false)
                    .with_world(0)
                    .with_supervisor_access(3)
                    .with_byte_address_select(0xf)
                    .with_enable(true),
            );
            dsb();

            // set up threads
            let cpsr: usize;
            core::arch::asm!("mrs {}, cpsr", out(reg) cpsr);
            let as_user_mode = (cpsr & !0x1f) | 0b10000;
            // set up registers
            self.threads[0].spsr = as_user_mode;
            self.threads[1].spsr = as_user_mode;
            self.threads[0].registers[0] = a as usize; // a0
            self.threads[1].registers[0] = b as usize;
            self.threads[0].registers[13] = 0x0f60_0000; // sp
            self.threads[1].registers[13] = 0x0f70_0000;
            self.threads[0].registers[15] = _interleave_wrap as usize; // pc
            self.threads[1].registers[15] = _interleave_wrap as usize;

            let p_box: Box<dyn AnyPolicy> = Box::new(p);

            self.policy = Some(p_box);

            core::arch::asm!(
                r#"
                bl 2f
                b 3f
                2:
                    str lr, [r0]
                    mov r0, #0
                    bx lr
                3:
                    cmp r0, #0
                    bne 4f
                    @ normal execution
                    mrs r0, cpsr
                    str r0, [r2, #4]
                    push {{r1-r12}}
                    ldr r0, [r1] @ spsr
                    msr spsr, r0
                    @ set R0-R12, R13_usr, R14_usr
                    @ note that R14_usr should not be read from
                    @ point LR_sup to [registers[15], spsr] in ThreadState so we can RFE into
                    @ _interleave_wrap
                    add lr, r2, #60
                    ldmia r2, {{r0-r14}}^
                    rfeia lr
                4:
                    pop {{r1-r12}}
                    @ okay, proceed as normal
                "#,
                inout("r0") self.threads[0].registers[14..15].as_mut_ptr() => _,
                in("r1") &raw const self.threads[1].spsr,
                in("r2") self.threads[0].registers.as_ptr(),
                out("lr") _,
            );

            steal_println!(
                "hashes: {:08x} {:08x}",
                self.threads[0].hash.take().unwrap().finalize(),
                self.threads[1].hash.take().unwrap().finalize()
            );

            self.policy
                .take()
                .expect("no policy")
                .as_any()
                .downcast_ref::<P>()
                .expect("wrong policy")
                .result()
        }
    }
    pub fn next_instruction(&mut self, lr: usize, sp: *mut [usize; 13]) -> usize {
        // Cleanup
        self.threads[self.current_thread].registers[0..13]
            .copy_from_slice(&unsafe { sp.read_volatile() });
        unsafe {
            core::arch::asm!(
                r#"#
                stmia {t}, {{r13, r14}}^
                mrs {t2}, spsr
                "#,
                t = in(reg) self.threads[self.current_thread].registers[13..=14].as_mut_ptr(),
                t2 = out(reg) self.threads[self.current_thread].spsr,
            );
        };
        self.threads[self.current_thread].registers[15] = lr;

        self.threads[self.current_thread]
            .hash
            .as_mut()
            .unwrap()
            .update(bytemuck::cast_slice(
                &self.threads[self.current_thread].registers,
            ));
        self.threads[self.current_thread]
            .hash
            .as_mut()
            .unwrap()
            .update(&self.threads[self.current_thread].spsr.to_le_bytes());

        // steal_println!("switched from {:x?}", self.threads[self.current_thread]);

        if lr == _interleave_stop as usize {
            self.threads[self.current_thread].did_stop = true;
        }

        let next_thread = self
            .policy
            .as_mut()
            .expect("no policy")
            .next_thread(self.current_thread, &self.threads)
            .unwrap_or(0);
        self.policy.as_mut().expect("no policy").check();

        // steal_println!("next_thread: {next_thread}");

        unsafe {
            core::ptr::copy_nonoverlapping(
                self.threads[next_thread].registers.as_ptr(),
                sp.as_mut_ptr(),
                13,
            );
            core::arch::asm!(
                r#"
                ldmia {t}, {{r13, r14}}^
                msr spsr, {t2}
                "#,
                t = in(reg) self.threads[next_thread].registers[13..=14].as_ptr(),
                t2 = in(reg) self.threads[next_thread].spsr
            );
        }
        let return_address = self.threads[next_thread].registers[15];

        // steal_println!("switching to {:x?}", self.threads[next_thread]);

        self.current_thread = next_thread;

        unsafe {
            dsb();
            breakpoint_value_register_0::write(Bvr(return_address as u32));
            dsb();
        }

        return_address
    }
}

static INTERLEAVE_SCHEDULER: SyncUnsafeCell<InterleaveScheduler> =
    SyncUnsafeCell::new(InterleaveScheduler::new());

extern "C" fn prefetch_abort(lr: usize, sp: *mut [usize; 13]) -> usize {
    let dscr = unsafe { debug_status_control::read() };
    let ifsr = unsafe { instruction_fault_status::read() };
    let far = unsafe { fault_address::read_raw() };
    let ifar = unsafe { instruction_fault_address::read_raw() };

    match DebugEntryMethod::try_from(dscr.debug_entry()) {
        Ok(DebugEntryMethod::BKPTInstr) => {
            steal_println!("PABT (bkpt) : dscr={dscr:x?} ifsr={ifsr:x?} lr={lr:08x}");

            lr + 4
        }
        Ok(_) => {
            // steal_println!(
            //     // "PABT : dscr={dscr:x?} ifsr={ifsr:x?} lr={lr:08x} far={far:08x} ifar={ifar:08x}"
            //     "PABT : lr={lr:08x} sp={:08x}",
            //     sp.addr()
            // );
            unsafe {
                INTERLEAVE_SCHEDULER
                    .get()
                    .as_mut_unchecked()
                    .next_instruction(lr, sp)
            }
        }
        Err(e) => {
            panic!("PABT : unknown DSCR entry value: {e}");
        }
    }
}
extern "C" fn data_abort(pc: usize) {
    let far = unsafe { fault_address::read_raw() };
    let dfsr = unsafe { data_fault_status::read_raw() };
    steal_println!("data_abort dfsr={dfsr:08x} pc={pc:08x} far={far:08x}");
}
extern "C" fn syscall(arg0: u32, arg1: u32, arg2: u32, imm: u32, arg3: u32, sp: u32, lr: u32) {
    // steal_println!("swi {imm} {arg0:08x} {arg1:08x} {arg2:08x} {arg3:08x} lr={lr:08x} sp={sp:08x}");

    if imm == SYS_ELEVATE {
        unsafe {
            // layout: r3 sp lr r0 r1 r2 lr spsr
            let saved_spsr: *mut u32 = core::ptr::with_exposed_provenance_mut(sp as usize + 16);
            let prev = saved_spsr.read();
            saved_spsr.write(arg0);
            dmb();
            // steal_println!(
            //     "set spsr @ {:08x} to {arg0:08x}, previous value: {prev:08x}",
            //     sp + 16
            // );
        }
    }
}

pub fn debug_init_singlestep() {
    let layout = Layout::array::<u32>(8).unwrap();
    let vdi_ptr: *mut u32 = core::ptr::with_exposed_provenance_mut(0);
    let vdi_len = layout.size();
    let vector_dst = core::ptr::slice_from_raw_parts_mut(vdi_ptr, vdi_len);

    dsb();
    unsafe {
        VectorBuilder::new()
            .set_data_abort_handler((&raw const _debug_data_abort_trampoline).addr())
            .set_prefetch_abort_handler((&raw const _debug_prefetch_abort_trampoline).addr())
            .set_syscall_handler((&raw const _debug_syscall_trampoline).addr())
            .install(vector_dst)
            .expect("#failed to install exception vector");
    };
    dsb();

    unsafe {
        unsafe { debug_status_control::modify(|dscr| dscr.with_monitor_debug_enable(true)) };
        prefetch_flush();
        dsb();

        int::init_stack_for_mode(OperatingMode::User, 0x0f10_0000);
        int::init_stack_for_mode(OperatingMode::Abort, 0x0f20_0000);
        int::init_stack_for_mode(OperatingMode::Undefined, 0x0f30_0000 - 8);
        int::init_stack_for_mode(OperatingMode::FIQ, 0x0f40_0000 - 8);
        int::init_stack_for_mode(OperatingMode::IRQ, 0x0f50_0000 - 8);

        steal_println!("initialized operating mode stacks");

        steal_println!("running interleave checker...");
        let mut result_set = vec![];
        for i in 0..10 {
            let result = INTERLEAVE_SCHEDULER.get().as_mut_unchecked().run(
                test_a,
                test_b,
                LabPolicyAba::new(i),
            );
            result_set.push(result);
        }
        result_set.sort();
        result_set.dedup();
        steal_println!("interleave results: {result_set:?}");

        dsb();
        breakpoint_control_register_0::write(Bcr(0));
        debug_status_control::modify(|dscr| dscr.with_monitor_debug_enable(false));
        dsb();

        steal_println!("done");
    }
}

struct LabPolicyAba {
    instruction_counter: usize,
    instruction_count_param: usize,
    is_ok: bool,
}
impl LabPolicyAba {
    pub fn new(instruction_count_param: usize) -> Self {
        Self {
            instruction_counter: 0,
            instruction_count_param,
            is_ok: true,
        }
    }
}
impl Policy for LabPolicyAba {
    fn next_thread(&mut self, _last_thread: usize, threads: &[ThreadState]) -> Option<usize> {
        let done0 = threads[0].did_stop;
        let done1 = threads[1].did_stop;
        let r = if self.instruction_counter < self.instruction_count_param && !done0 {
            Some(0)
        } else if !done1 {
            Some(1)
        } else if !done0 {
            Some(0)
        } else {
            None
        };
        // steal_println!(
        //     "next_thread: done0={done0:?} done1={done1:?} ic={} icp={} r={r:?}",
        //     self.instruction_counter,
        //     self.instruction_count_param
        // );
        self.instruction_counter += 1;
        r
    }
    fn check(&mut self) {
        if unsafe { LOCK > 1 } {
            self.is_ok = false;
        }
    }
}
impl PolicyExt for LabPolicyAba {
    type Result = bool;
    fn result(&self) -> Self::Result {
        self.is_ok
    }
    fn init(&self) {
        unsafe { LOCK = 0 };
    }
}

static mut LOCK: usize = 0;
fn trylock() -> bool {
    let x = unsafe { LOCK };
    if x > 0 {
        false
    } else {
        unsafe {
            (&raw mut LOCK).write_volatile((&raw mut LOCK).read_volatile() + 1);
        }
        true
    }
}
fn unlock() {
    unsafe { LOCK -= 1 };
}

#[unsafe(no_mangle)]
pub fn test_a() {
    if trylock() {
        unsafe {
            core::arch::asm!("nop");
        }
        unlock();
    }
}
#[unsafe(no_mangle)]
pub fn test_b() {
    if trylock() {
        unsafe {
            core::arch::asm!("nop");
        }
    }
}

pub fn interleave_checker() {
    debug_init_singlestep();
}
