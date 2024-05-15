use core::marker::PhantomData;

const PROGRESS_BIT: u32 = 0x0000_0100;
const SEQUENCE_MASK: u32 = 0xffff_ff00;

#[derive(Debug)]
#[repr(transparent)]
pub struct HybridSeqLock(u32);

#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct SeqNo(u32);

impl SeqNo {
    pub fn write_in_progress(&self) -> bool {
        (self.0 & PROGRESS_BIT) != 0
    }
}

impl HybridSeqLock {
    pub fn new() -> Self { Self(0) }
    pub fn read(&self) -> SeqNo {
        let v = unsafe {
            core::intrinsics::atomic_load_acquire(core::ptr::from_ref(&self.0))
        };
        SeqNo(v & SEQUENCE_MASK)
    }
    pub fn try_acquire_write_lock(&self) -> Option<WriteSentinel> {
        let prev = unsafe { core::intrinsics::atomic_xchg_acqrel(core::ptr::from_ref(&self.0).cast_mut(), 1) };
        if prev == 0 {
            Some(WriteSentinel {
                inner: core::ptr::from_ref(&self.0).cast_mut(),
                progress: false,
                _marker: Default::default()
            })
        } else {
            None
        }
    }
}

pub struct WriteSentinel<'a> {
    inner: *mut u32,
    progress: bool,
    _marker: PhantomData<&'a u32>,
}

impl<'a> WriteSentinel<'a> {
    pub fn progress(&mut self) {
        assert!(!self.progress, "Already progress()'d WriteSentinel on {:p}", &self.inner);
        unsafe { core::intrinsics::atomic_xchg_acqrel(self.inner, PROGRESS_BIT) };
        self.progress = true;
    }
}

impl<'a> Drop for WriteSentinel<'a> {
    fn drop(&mut self) {
        if self.progress {
            unsafe { core::intrinsics::atomic_xadd_acqrel(self.inner, PROGRESS_BIT) };
        }
        unsafe { core::intrinsics::atomic_store_release(self.inner.cast::<u8>(), 0) };
    }
}
