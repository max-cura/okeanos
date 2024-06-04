use crate::arch::arm1176::cpsr::{ExecutionState, __read_cpsr, __write_cpsr};
use crate::arch::arm1176::sync::{__read_tpidrprw, __write_tpidrprw};
use crate::sync::once::OnceLockInit;
use crate::sync::ticket::TicketLock;
use crate::uart1_sendln_bl;
use alloc::boxed::Box;
use core::alloc::Layout;
use core::arch::asm;
use core::hint::unreachable_unchecked;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use proc_bitfield::UnsafeInto;
use slice_dst::SliceWithHeader;

pub struct ThreadContext {
    opaque_ptr: *mut (),
}

#[repr(C)]
pub struct ThreadControlBlock {
    register_file: RegisterFile,
    t_id: u32,
    stack_base: *mut u32,
    stack_size: usize,
    tls_block: Box<SliceWithHeader<StaticTLSBlockHeader, u32>>,

    next: Option<NonNull<ThreadControlBlock>>,
    prev: Option<NonNull<ThreadControlBlock>>,

    entry_point: fn(),
}

struct GTLInner(Option<NonNull<ThreadControlBlock>>);

pub struct GlobalThreadList {
    list: TicketLock<GTLInner>,
    thread_count: AtomicUsize,
    thread_id: AtomicU32,
}
unsafe impl Sync for GlobalThreadList {}

impl GlobalThreadList {
    pub const fn new() -> GlobalThreadList {
        Self {
            list: TicketLock::new(GTLInner(None)),
            thread_count: AtomicUsize::new(1),
            thread_id: AtomicU32::new(0),
        }
    }
    pub fn initialize(
        &self,
        k_stack_start: *mut u32,
        k_stack_size: usize,
        static_tls_slots: usize,
    ) {
        let tls_block = StaticTLSBlockHeader::new_static(static_tls_slots);
        let mut tcb = ThreadControlBlock {
            register_file: RegisterFile::zeroed(),
            t_id: self.thread_id.fetch_add(1, Ordering::Relaxed),
            stack_base: k_stack_start,
            stack_size: k_stack_size,
            tls_block,
            next: None,
            prev: None,
            entry_point: || {},
        };
        let mut in_mem = Box::into_raw(Box::new(tcb));
        unsafe {
            (&mut *in_mem).next = Some(NonNull::new_unchecked(in_mem));
            (&mut *in_mem).prev = Some(NonNull::new_unchecked(in_mem));
            __write_tpidrprw(in_mem as usize as u32);
        }
        {
            unsafe {
                let mut guard = self.list.lock();
                *guard = GTLInner(Some(NonNull::new_unchecked(in_mem)));
            }
        }
        uart1_sendln_bl!("[bis]: Attempting to initialize thread 0...");
        // unsafe {
        //     asm!(
        //     "svc 0",
        //     in("r0") 0,
        //     )
        // }
        unsafe { __bis_yield_from_generic_super() }
        uart1_sendln_bl!("[bis]: Attempting to initialize thread 0... success");
    }

    pub fn thread_create(
        &self,
        stack_start: *mut u32,
        stack_size: usize,
        static_tls_slots: usize,
        entry_point: fn(),
        execution_state: ExecutionState,
    ) {
        let tls_block = StaticTLSBlockHeader::new_static(static_tls_slots);
        let mut tcb = ThreadControlBlock {
            register_file: RegisterFile::zeroed(),
            t_id: self.thread_id.fetch_add(1, Ordering::Relaxed),
            stack_base: stack_start,
            stack_size,
            tls_block,
            next: None,
            prev: None,
            entry_point,
        };
        self.thread_count.fetch_add(1, Ordering::AcqRel);
        tcb.register_file.general[15] = entry_point as *mut u32 as u32;
        tcb.register_file.general[14] = __bis_on_thread_exit as *mut u32 as u32;
        tcb.register_file.general[13] =
            unsafe { stack_start.byte_offset(stack_size as isize - 4) as usize as u32 };
        let spsr = __read_cpsr().with_execution_state(execution_state);
        tcb.register_file.spsr = unsafe { spsr.unsafe_into() };
        let mut in_mem = Box::into_raw(Box::new(tcb));
        unsafe {
            let mut guard = self.list.lock();
            let head = guard.0.unwrap();
            (&mut *in_mem).next = Some(head);
            (&mut *in_mem).prev = head.as_ref().prev;
            (&mut *in_mem).prev.unwrap().as_mut().next = Some(NonNull::new_unchecked(in_mem));
            (&mut *in_mem).next.unwrap().as_mut().prev = Some(NonNull::new_unchecked(in_mem));
            *guard = GTLInner(Some(NonNull::new_unchecked(in_mem)));
        }
    }
}

#[no_mangle]
extern "C" fn __bis_thread_enter() -> ! {
    let tcb = __read_tpidrprw() as usize as *mut ThreadControlBlock;

    uart1_sendln_bl!("__bis_thread_enter with {tcb:p}");
    unsafe {
        ((&*tcb).entry_point)();
    }
    __bis_on_thread_exit();
}

pub static THREAD_CHAIN: OnceLockInit<GlobalThreadList, fn() -> GlobalThreadList> =
    OnceLockInit::new(|| GlobalThreadList::new());

struct RegisterFile {
    // r0-r12, r13_sp, r14_lr, r15_pc
    pub general: [u32; 16],
    pub spsr: u32,
}

impl RegisterFile {
    fn zeroed() -> RegisterFile {
        Self {
            general: [0; 16],
            spsr: 0,
        }
    }
}

#[repr(C)]
pub struct StaticTLSBlockHeader {
    dtv: Box<DynamicThreadVector>,
    reserved: u32,
}
impl StaticTLSBlockHeader {
    pub fn new_static(slot_count: usize) -> Box<SliceWithHeader<StaticTLSBlockHeader, u32>> {
        let dtv = Box::new(DynamicThreadVector {
            generation: AtomicU32::new(0),
            modules: [ptr::null_mut()],
        });
        let mut static_tls = SliceWithHeader::new::<Box<SliceWithHeader<Self, u32>>, _>(
            StaticTLSBlockHeader { dtv, reserved: 0 },
            core::iter::repeat_n(0u32, slot_count),
        );
        static_tls.header.dtv.modules[0] = unsafe { static_tls.slice.as_mut_ptr().offset(0) };

        static_tls
    }
}

#[repr(C)]
pub struct DynamicThreadVector {
    generation: AtomicU32,
    // for now, it's fixed
    modules: [*mut u32; 1],
}

extern "C" {
    pub fn __bis_yield_from_irq();
    pub fn __bis_yield_from_swi();
    pub fn __bis_yield_from_generic_super();
    pub fn __bis_switch_back_user(r0: *mut ThreadControlBlock) -> !;
    pub fn __bis_switch_back_priv(r0: *mut ThreadControlBlock) -> !;
}

unsafe fn __bis_switch_back(r0: *mut ThreadControlBlock) -> ! {
    if ((&*r0).register_file.spsr & 0x1f) == 0x10 {
        __bis_switch_back_user(r0)
    } else {
        __bis_switch_back_priv(r0)
    }
}

#[no_mangle]
pub extern "C" fn __bis_thread_cleanup() -> ! {
    let tcb = __read_tpidrprw() as usize as *mut ThreadControlBlock;
    let tcb_ref = unsafe { &*tcb };
    let tid = tcb_ref.t_id;
    uart1_sendln_bl!("[bis]: thread {} exited, cleaning up", tid);

    // Remove from the thread chain
    let tcb_mut = unsafe { &mut *tcb };
    unsafe {
        let g = THREAD_CHAIN.get().list.lock();

        tcb_mut.next.unwrap().as_mut().prev = tcb_mut.prev;
        tcb_mut.prev.unwrap().as_mut().next = tcb_mut.next;
        THREAD_CHAIN
            .get()
            .thread_count
            .fetch_sub(1, Ordering::AcqRel);
    }

    // Deallocate the stack
    unsafe {
        alloc::alloc::dealloc(
            tcb_mut.stack_base.cast(),
            Layout::from_size_align(tcb_mut.stack_size, 4).unwrap(),
        )
    }
    // Deallocate the TCB
    let _ = unsafe { Box::from_raw(tcb) };

    let next = tcb_mut.next.unwrap().as_ptr();
    unsafe { __bis_switch_back(next) }
}

pub extern "C" fn __bis_on_thread_exit() -> ! {
    uart1_sendln_bl!("__bis_on_thread_exit");
    if __read_cpsr().execution_state() == ExecutionState::User {
        unsafe {
            asm!(
            "swi 0",
            in("r0") 1,
            options(noreturn),
            )
        }
    } else {
        panic!("[bis]: non-user thread exited");
    }
}

#[no_mangle]
pub extern "C" fn __bis_switch_context(tcb: *mut ThreadControlBlock, magic: u32) -> ! {
    // unsafe {
    //     asm!(
    //     "wfi",
    //     "mov r5, {t}",
    //     "wfi",
    //     t = in(reg) &(&*tcb).register_file.general[0],
    //     out("r5") _,
    //     )
    // }
    dump_regs();

    assert!(magic >= 1 && magic <= 3);
    if magic == 1 {
        uart1_sendln_bl!("__bis_ctx_switch called from IRQ");
    } else if magic == 2 {
        uart1_sendln_bl!("__bis_ctx_switch called from SWI");
    } else if magic == 3 {
        uart1_sendln_bl!("__bis_ctx_switch called from kernel thread");
    }

    uart1_sendln_bl!("tpidrprw: {:08x}", __read_tpidrprw());

    uart1_sendln_bl!("tcb: {tcb:p}");

    let tcb_ref = unsafe { &*tcb };

    for i in 0..16 {
        uart1_sendln_bl!("saved r{i}={:08x}", tcb_ref.register_file.general[i]);
    }
    uart1_sendln_bl!("saved spsr={:08x}", tcb_ref.register_file.spsr);

    unsafe {
        let new_tcb = (&*tcb).next.unwrap();
        {
            let tcb_ref = new_tcb.as_ref();
            for i in 0..16 {
                uart1_sendln_bl!("saved r{i}={:08x}", tcb_ref.register_file.general[i]);
            }
            uart1_sendln_bl!("saved spsr={:08x}", tcb_ref.register_file.spsr);
        }
        __write_tpidrprw(new_tcb.as_ptr() as usize as u32);
        __bis_switch_back(new_tcb.as_ptr());
    }
}

pub fn dump_regs() {
    let mut regs: [u32; 16] = [0; 16];
    unsafe {
        asm!(
                "str r1, [r0, #4]",
                "str r2, [r0, #8]",
                "str r3, [r0, #12]",
                "str r4, [r0, #16]",
                "str r5, [r0, #20]",
        // "str r6, [r0, #24]",
                "str r7, [r0, #28]",
                "str r8, [r0, #32]",
                "str r9, [r0, #36]",
                "str r10, [r0, #40]",
        // "str r11, [r0, #44]",
                "str r12, [r0, #48]",
                "str r13, [r0, #52]", // sp
                "str r14, [r0, #56]", // lr
                in("r0") regs.as_mut_ptr(),
                )
    }
    uart1_sendln_bl!("==== REGISTER DUMP ====");
    for i in 0..16 {
        if i == 0 || i == 6 || i == 11 || i == 15 {
            uart1_sendln_bl!("r{i}=XXXXXXXX");
        } else {
            uart1_sendln_bl!("r{i}={:08x}", regs[i]);
        }
    }
}
