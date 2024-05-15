use core::cell::UnsafeCell;
use core::ptr;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicPtr, AtomicUsize};
use core::sync::atomic::Ordering::{AcqRel, Acquire};
use lock_api::RawMutex;
use crate::arch::arm1176::sync::ticket::RawTicketLock;

union Item<T> {
    list_next: Option<NonNull<Item<T>>>,
    object: T,
}

pub struct Arena<T> {
    alloc_list: UnsafeCell<Option<NonNull<Item<T>>>>,
    free_list: AtomicPtr<Item<T>>,
    count_free: AtomicUsize,
    lock: RawTicketLock,
    arena_begin: *mut Item<T>,
    arena_end: *mut Item<T>,
}

impl<T> Arena<T> {
    pub fn allocate(&self) -> Option<NonNull<T>> {
        self.lock.lock();

        let alloc_head = match unsafe { *self.alloc_list.get() } {
            None => {
                unsafe { self.alloc_list.get().write(NonNull::new(self.free_list.swap(ptr::null_mut(), AcqRel))) };
                *(unsafe { &*self.alloc_list.get() }).as_ref()?
            },
            Some(l) => l,
        };
        let next = unsafe { alloc_head.as_ref() }.list_next;
        self.count_free.fetch_sub(1, AcqRel);
        unsafe { *self.alloc_list.get() = next };

        self.lock.unlock();

        NonNull::new(ptr::from_ref(&unsafe { alloc_head.as_ref() }.object).cast_mut())
    }
    pub fn deallocate(&self, ptr: NonNull<T>) {
        if unsafe {
            ptr.as_ptr().byte_offset_from(self.arena_begin) < 0
                || self.arena_end.byte_offset_from(ptr.as_ptr()) < 0
                || !ptr.as_ptr().is_aligned()
        } {
            return
        }
        let mut ptr_as_item = ptr.cast::<Item<T>>();
        let mut current_head = self.free_list.load(Acquire);
        loop {
            unsafe {
                let mref = ptr_as_item.as_mut();
                mref.list_next = NonNull::new(current_head);
            };
            match self.free_list.compare_exchange_weak(current_head, ptr_as_item.as_ptr(), AcqRel, Acquire) {
                Ok(_prev) => break,
                Err(actually) => current_head = actually,
            }
            self.count_free.fetch_add(1, AcqRel);
        }
    }
}