//! Kernel-level threads of execution

use crate::steal_println;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ptr;
use core::ptr::NonNull;
use quartz::arch::arm1176::sync::ticket::TicketLock;
use quartz::arch::arm1176::tpid::{__read_tpidrprw, __write_tpidrprw};
use thiserror::Error;

pub type ThreadId = usize;

#[repr(C)]
pub struct ThreadHeader {
    register_file: RegisterFile,
    id: ThreadId,
    stack_start: u32,
    stack_end: u32,
    tls_block: *mut u32, // unused
    entry: Box<Entry>,
}
pub struct Entry {
    entry_point: Box<dyn FnOnce()>,
}
#[derive(Debug)]
#[repr(C)]
struct RegisterFile {
    registers: [u32; 16],
    cpsr: u32,
}

/// This function is run to initialize the thread.
#[unsafe(no_mangle)]
pub extern "C" fn _bis_thread_enter() {
    let th: *mut ThreadHeader = core::ptr::with_exposed_provenance_mut(__read_tpidrprw() as usize);
    // steal_println!("_bis_on_thread_enter({th:#?})");
    unsafe {
        (th.read_volatile().entry.entry_point)();
    }
    _bis_on_thread_exit(th);
}

/// This function cleans up the thread context
#[unsafe(no_mangle)]
pub extern "C" fn _bis_on_thread_exit(th: *mut ThreadHeader) {
    // steal_println!("_bis_on_thread_exit({th:#?})");
    let next_th = SCHEDULER.kill(th);
    // steal_println!("next TH: {next_th:#?}, regfile={:08x?}", unsafe {
    //     &(&*next_th).register_file
    // });
    unsafe {
        __write_tpidrprw(next_th.expose_provenance() as u32);
        _bis_resume(next_th.expose_provenance());
        panic!("_bis_resume returned");
    }
}

unsafe extern "C" {
    fn _bis_yield_from_super();
    fn _bis_resume(th: usize);
}
#[unsafe(no_mangle)]
pub extern "C" fn _bis_switch_from_super(th: *mut ThreadHeader) {
    // steal_println!("_bis_switch_from_super({th:#?}), regfile={:08x?}", unsafe {
    //     &(&*th).register_file
    // });
    // register file has been saved
    let next_th = SCHEDULER.switch(th);
    // steal_println!("next TH: {next_th:#?}, regfile={:08x?}", unsafe {
    //     &(&*next_th).register_file
    // });
    unsafe {
        __write_tpidrprw(next_th.expose_provenance() as u32);
        _bis_resume(next_th.expose_provenance());
        panic!("_bis_resume returned");
    }
}

static SCHEDULER: Scheduler = Scheduler::new();

struct ScheduleItem {
    th: NonNull<ThreadHeader>,
    next: NonNull<ScheduleItem>,
    prev: NonNull<ScheduleItem>,
}

struct Scheduler {
    inner: UnsafeCell<Inner>,
}
impl Scheduler {
    const fn new() -> Self {
        Self {
            inner: UnsafeCell::new(Inner::new()),
        }
    }
    fn spawn<F: FnOnce() + Send + 'static>(&self, f: F, stack_size: usize) -> ThreadId {
        unsafe { &mut *self.inner.get() }.spawn(f, stack_size)
    }
    fn enter(&self) {
        unsafe { &mut *self.inner.get() }.enter()
    }
    fn kill(&self, th: *mut ThreadHeader) -> *mut ThreadHeader {
        unsafe { &mut *self.inner.get() }.kill(th)
    }
    fn switch(&self, th: *mut ThreadHeader) -> *mut ThreadHeader {
        unsafe { &mut *self.inner.get() }.switch(th)
    }
}
unsafe impl Sync for Scheduler {}

struct Inner {
    current: Option<NonNull<ScheduleItem>>,
    next_id: u32,
}
impl Inner {
    const fn new() -> Self {
        Self {
            current: None,
            next_id: 1,
        }
    }
    fn spawn<F: FnOnce() + Send + 'static>(&mut self, f: F, stack_size: usize) -> ThreadId {
        let (begin, len, _cap) = Vec::into_raw_parts(vec![0u8; stack_size]);
        let stack_start = begin.expose_provenance();
        let mut register_file = RegisterFile {
            registers: [0; 16],
            cpsr: 0x13,
        };
        register_file.registers[13] = (stack_start + len) as u32;
        register_file.registers[14] = _bis_on_thread_exit as usize as u32;
        register_file.registers[15] = _bis_thread_enter as usize as u32;
        let tid = {
            let id = self.next_id;
            self.next_id += 1;
            id as ThreadId
        };
        let thnn = Box::into_non_null(Box::new(ThreadHeader {
            register_file,
            id: tid,
            stack_start: stack_start as u32,
            stack_end: (stack_start + len) as u32,
            tls_block: ptr::null_mut(),
            entry: Box::new(Entry {
                entry_point: Box::new(f),
            }),
        }));
        let mut sinn = Box::into_non_null(Box::new(ScheduleItem {
            th: thnn,
            next: NonNull::dangling(),
            prev: NonNull::dangling(),
        }));
        if let Some(mut c) = self.current {
            let c_ref = unsafe { c.as_mut() };
            unsafe {
                sinn.as_mut().next = c_ref.next;
                sinn.as_mut().prev = c;
                c_ref.next.as_mut().prev = sinn;
                c.as_mut().next = sinn;
            }
        } else {
            unsafe {
                sinn.as_mut().next = sinn;
                sinn.as_mut().prev = sinn;
            }
        }
        // self.current = Some(sinn);
        tid
    }
    fn switch(&mut self, th: *mut ThreadHeader) -> *mut ThreadHeader {
        let next = {
            let current = self.current.unwrap();
            let si_ref = unsafe { current.as_ref() };
            assert_eq!(th, si_ref.th.as_ptr());
            let next = si_ref.next;
            next
        };
        self.current = Some(next);
        let new_th = unsafe { next.as_ref() }.th;
        new_th.as_ptr()
    }
    fn kill(&mut self, th: *mut ThreadHeader) -> *mut ThreadHeader {
        let mut current = self.current.unwrap();
        let (mut next, is_last) = {
            let si_ref = unsafe { current.as_ref() };
            let is_last = si_ref.next == current;
            assert_eq!(th, si_ref.th.as_ptr());
            let next = si_ref.next;
            (next, is_last)
        };
        if is_last {
            panic!("last thread killed by Scheduler");
        }
        unsafe {
            current.as_mut().prev.as_mut().next = next;
            next.as_mut().prev = current.as_ref().prev;
        }
        self.current = Some(next);
        unsafe { next.as_ref().th.as_ptr() }
    }
    fn enter(&mut self) {
        let thnn = Box::into_non_null(Box::new(ThreadHeader {
            register_file: RegisterFile {
                registers: [0; 16],
                cpsr: 0,
            },
            id: 0,
            stack_start: 0,
            stack_end: 0,
            tls_block: ptr::null_mut(),
            entry: Box::new(Entry {
                entry_point: Box::new(|| {}),
            }),
        }));
        let mut sinn = Box::into_non_null(Box::new(ScheduleItem {
            th: thnn,
            next: NonNull::dangling(),
            prev: NonNull::dangling(),
        }));
        steal_println!("thnn: {thnn:?}, sinn_th: {:?}", unsafe { sinn.as_ref().th });
        if let Some(mut c) = self.current {
            let c_ref = unsafe { c.as_mut() };
            unsafe {
                sinn.as_mut().next = c_ref.next;
                sinn.as_mut().prev = c;
                c_ref.next.as_mut().prev = sinn;
                c.as_mut().next = sinn;
            }
        } else {
            unsafe {
                sinn.as_mut().next = sinn;
                sinn.as_mut().prev = sinn;
            }
        }
        self.current = Some(sinn);
        unsafe { __write_tpidrprw(thnn.as_ptr().expose_provenance() as u32) };
    }
}

pub fn yield_now() {
    unsafe { _bis_yield_from_super() }
}

pub struct JoinHandle<U: Send + 'static> {
    _tid: ThreadId,
    rv: Arc<TicketLock<Option<U>>>,
}
#[derive(Debug, Error)]
pub enum JoinError {
    //
}
impl<U: Send + 'static> JoinHandle<U> {
    pub fn wait(&self) -> Result<U, JoinError> {
        loop {
            {
                let mut lg = self.rv.lock();
                if let Some(rv) = lg.take() {
                    return Ok(rv);
                }
                let _ = lg;
            }
            yield_now();
        }
    }
}

pub const STACK_SIZE_DEFAULT: usize = 0x10000;

pub fn init() {
    SCHEDULER.enter();
}

pub fn spawn<U: Send + 'static, F: FnOnce() -> U + Send + 'static>(f: F) -> JoinHandle<U> {
    let rv1: Arc<TicketLock<Option<U>>> = Arc::new(TicketLock::new(None));
    let rv2 = rv1.clone();
    let wrapper = move || {
        let out = f();
        let mut lg = rv2.lock();
        let _ = lg.insert(out);
    };
    let _tid = SCHEDULER.spawn(wrapper, STACK_SIZE_DEFAULT);
    JoinHandle { _tid, rv: rv1 }
}
