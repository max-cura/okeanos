use core::sync::atomic::{AtomicU32, Ordering};

const PROGRESS_BIT : u32 = 1;

#[derive(Debug)]
#[repr(transparent)]
pub struct SeqLock(AtomicU32);

#[derive(Debug, Copy, Clone, Eq, PartialOrd, PartialEq)]
#[repr(transparent)]
pub struct SeqNo(u32);

impl SeqNo {
    pub fn write_in_progress(&self) -> bool {
        (self.0 & PROGRESS_BIT) != 0
    }
}

impl SeqLock {
    pub fn new() -> Self { Self(AtomicU32::new(0)) }
    pub fn read(&self) -> SeqNo {
        SeqNo(self.0.load(Ordering::Acquire))
    }
    pub unsafe fn write_lock_unchecked(&self) -> WriteSentinel {
        self.0.fetch_add(PROGRESS_BIT, Ordering::AcqRel);
        WriteSentinel {
            inner: self
        }
    }
}

pub struct WriteSentinel<'a> {
    inner: &'a SeqLock,
}

impl<'a> Drop for WriteSentinel<'a> {
    fn drop(&mut self) {
        self.fetch_add(PROGRESS_BIT, Ordering::AcqRel)
    }
}
