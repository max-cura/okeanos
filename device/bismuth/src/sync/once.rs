use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::arch::arm1176::{__sev, __wfe};

const UNINIT : u32 = 0;
const RUNNING : u32 = 1;
const READY : u32 = 2;

pub struct OnceLock<T> {
    state: AtomicU32,
    inner: UnsafeCell<MaybeUninit<T>>,
}

unsafe impl<T> Sync for OnceLock<T> {}

impl<T> OnceLock<T> {
    pub const fn new() -> Self {
        Self {
            state: AtomicU32::new(UNINIT),
            inner: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
    pub fn get(&self) -> Option<&T> {
        (self.state.load(Ordering::Acquire) == READY)
            .then(|| unsafe{ (&*self.inner.get()).assume_init_ref() })
    }
    pub fn set(&self, inner: T) -> Result<(), T> {
        if self.state.compare_exchange(UNINIT, RUNNING, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            unsafe { (&mut*self.inner.get()).write(inner) };
            Ok(())
        } else {
            Err(inner)
        }
    }
    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
        match self.state.compare_exchange(UNINIT, RUNNING, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                unsafe { (&mut*self.inner.get()).write(f()) };
                self.state.store(READY, Ordering::Release);
                __sev();
            }
            Err(_) => {
                while self.state.load(Ordering::Acquire) != READY {
                    __wfe();
                }
            }
        }
        unsafe { (&*self.inner.get()).assume_init_ref() }
    }
}