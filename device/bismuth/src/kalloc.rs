use crate::sync::ticket::{RawTicketLock, TicketLock};
use crate::uart1_sendln_bl;
use core::alloc::{AllocError, Allocator, GlobalAlloc, Layout};
use core::mem::{align_of, size_of, MaybeUninit};
use core::ops::Range;
use core::ptr;
use core::ptr::NonNull;
use lock_api::{MutexGuard, RawMutex};

pub struct ObjectHeader {
    pub(crate) size: usize,
    pub flags: u32,
    prev: Option<NonNull<ObjectHeader>>,
    next: Option<NonNull<ObjectHeader>>,
}

impl ObjectHeader {
    pub unsafe fn data_ptr(&self) -> *mut u8 {
        ptr::from_ref(self).offset(1).cast::<u8>().cast_mut()
    }
    // returns true if self is the smallest ObjectHeader that can still fit `layout`.
    fn min_fit_for(&self, layout: Layout) -> bool {
        self.size == layout.size().next_multiple_of(align_of::<Self>())
    }
    fn try_layout(&self, layout: Layout) -> ObjectCut {
        let size = layout.size();
        let align = layout.align();
        if size > self.size {
            return ObjectCut::None;
        }
        if align <= align_of::<Self>() {
            let size_actual = layout.size().next_multiple_of(align_of::<Self>());
            if self.size <= (size_actual + size_of::<Self>()) {
                ObjectCut::One
            } else {
                let c1 = size.next_multiple_of(align_of::<Self>());
                let c2 = self.size - c1 - size_of::<Self>();
                ObjectCut::Two(c1, c2)
            }
        } else {
            assert_eq!(align % align_of::<Self>(), 0);
            // alignment greater than alignment of ObjectHeader
            // in which case; right now, we do this super stupid
            let first_align = unsafe { self.data_ptr() }.align_offset(align);
            if (first_align + size) <= self.size {
                ObjectCut::Two(first_align - size_of::<Self>(), self.size - first_align)
            } else {
                ObjectCut::None
            }
            // panic!("alignment too high for SimpleAlloc: {align}");
        }
    }
    pub fn as_alloc_block(&self) -> AllocBlock {
        AllocBlock {
            header: ptr::from_ref(self).cast::<u8>().cast_mut(),
            data: unsafe { self.data_ptr() },
            size: self.size,
        }
    }
    pub fn contains_ptr(&self, p: *mut u8) -> bool {
        let data = unsafe { self.data_ptr() };
        data <= p && p < unsafe { data.byte_offset(self.size as isize) }
    }
}

pub struct AllocBlock {
    pub header: *mut u8,
    pub data: *mut u8,
    pub size: usize,
}

enum ObjectCut {
    None,
    One,
    Two(usize, usize),
}

pub struct SimpleAlloc {
    free_objects: TicketLock<Option<NonNull<ObjectHeader>>>,
    allocated_objects: TicketLock<Option<NonNull<ObjectHeader>>>,

    ptr: *mut u8,
    len: usize,
}

unsafe impl Sync for SimpleAlloc {}

impl SimpleAlloc {
    pub fn new(ptr: *mut u8, len: usize) -> Self {
        assert!(ptr.is_aligned_to(core::mem::align_of::<ObjectHeader>()));
        assert!(len >= core::mem::align_of::<ObjectHeader>());
        let nonnull = NonNull::new(ptr).expect("SimpleAlloc::new() needs a non-null pointer");
        let mut hdr = nonnull.cast::<MaybeUninit<ObjectHeader>>();
        unsafe { hdr.as_mut() }.write(ObjectHeader {
            size: len - core::mem::size_of::<ObjectHeader>(),
            flags: 0,
            next: None,
            prev: None,
        });
        let hdr = hdr.cast::<ObjectHeader>();
        Self {
            free_objects: TicketLock::new(Some(hdr)),
            allocated_objects: TicketLock::new(None),
            ptr,
            len,
        }
    }

    pub fn iter_free_objects(&self) -> ObjectIter {
        let g = self.free_objects.lock();
        let curr = *g;
        ObjectIter { curr }
    }
    pub fn iter_allocated_objects(&self) -> ObjectIter {
        let g = self.allocated_objects.lock();
        let curr = *g;
        ObjectIter { curr }
    }

    pub fn range(&self) -> Range<*mut u8> {
        self.ptr..unsafe { self.ptr.byte_offset(self.len as isize) }
    }
}

pub struct ObjectIter {
    curr: Option<NonNull<ObjectHeader>>,
}

impl Iterator for ObjectIter {
    type Item = NonNull<ObjectHeader>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.curr {
            Some(oh) => {
                self.curr = unsafe { oh.as_ref() }.next;
                Some(oh)
            }
            None => None,
        }
    }
}

unsafe impl GlobalAlloc for SimpleAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut free_objects = self.free_objects.lock();

        let mut prev: Option<NonNull<ObjectHeader>> = None;
        let mut curr = match *free_objects {
            None => {
                return ptr::null_mut();
            }
            Some(p) => p,
        };
        let mut oh = loop {
            match curr.as_ref().try_layout(layout) {
                ObjectCut::None => {}
                ObjectCut::One => {
                    match prev {
                        Some(mut prev) => {
                            prev.as_mut().next = curr.as_ref().next;
                        }
                        None => {
                            *free_objects = curr.as_ref().next;
                        }
                    }
                    break curr;
                }
                ObjectCut::Two(c1, c2) => {
                    assert_eq!(c1 + c2 + size_of::<ObjectHeader>(), curr.as_ref().size);
                    let h2 = curr
                        .as_ref()
                        .data_ptr()
                        .byte_offset(c1.next_multiple_of(align_of::<ObjectHeader>()) as isize)
                        .cast::<MaybeUninit<ObjectHeader>>();
                    h2.as_mut().unwrap().write(ObjectHeader {
                        next: curr.as_ref().next,
                        flags: 0,
                        size: c2,
                        prev: None,
                    });
                    curr.as_mut().size = c1;
                    match prev {
                        Some(mut prev) => {
                            prev.as_mut().next = Some(NonNull::new(h2).unwrap().cast());
                        }
                        None => {
                            *free_objects = Some(NonNull::new(h2).unwrap().cast());
                        }
                    }
                    break curr;
                }
            }
            prev = Some(curr);
            curr = match curr.as_ref().next {
                None => return ptr::null_mut(),
                Some(p) => p,
            };
        };
        let mut g = self.allocated_objects.lock();
        oh.as_mut().prev = None;
        oh.as_mut().next = *g;
        assert_eq!(oh.as_ref().flags & 1, 0);
        oh.as_mut().flags |= 1;
        *g = Some(oh);
        // force drop order
        let _ = g;
        let _ = free_objects;

        oh.as_ref().data_ptr()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        let off = ptr.align_offset(align_of::<ObjectHeader>());
        let header = ptr
            .offset(off as isize)
            .cast::<ObjectHeader>()
            .offset(if off > 0 { -2 } else { -1 });
        // uart1_sendln_bl!("freeing object @ off={off} {ptr:p}, header at {header:p}");
        let header = &mut *header;
        assert_eq!(header.flags & 1, 1);
        let mut fo_guard = self.free_objects.lock();
        let mut g = self.allocated_objects.lock();
        match header.prev {
            Some(mut p) => {
                p.as_mut().next = header.next;
            }
            None => {
                *g = header.next;
            }
        }
        match header.next {
            Some(mut p) => {
                p.as_mut().prev = header.prev;
            }
            None => {}
        }
        header.flags &= !1;
        header.next = *fo_guard;
        *fo_guard = Some(NonNull::from(header));
    }
}
