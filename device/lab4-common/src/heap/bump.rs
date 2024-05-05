use core::alloc::Layout;
use core::ptr::NonNull;

#[derive(Debug)]
pub struct BumpHeap {
    start: *mut u8,
    end: *mut u8,

    ptr: *mut u8,
}

impl BumpHeap {
    pub fn new(start: *mut u8, end: *mut u8) -> Self {
        Self { start, end, ptr: start }
    }

    pub fn highest(&self) -> *mut u8 {
        self.ptr
    }

    pub fn allocate(&mut self, layout: Layout) -> Option<NonNull<[u8]>> {
        let size = layout.size();
        let align = layout.align();

        // note that this is in bytes only because ptr: *mut u8
        let ptr_align_bytes = self.ptr.align_offset(align);
        if ptr_align_bytes == usize::MAX {
            panic!("ptr_align_bytes={ptr_align_bytes}");
            return None
        }
        let alloc_ptr = (self.ptr as usize)
            .checked_add(ptr_align_bytes)?
            as *mut u8;

        let new_ptr = (alloc_ptr as usize)
            .checked_add(size)?
            as *mut u8;

        if new_ptr >= self.end {
            panic!("new_ptr={new_ptr:#?}, self.end={:#?}", self.end);
            return None
        } else {
            self.ptr = new_ptr;
            NonNull::new(alloc_ptr)
                .map(|p| NonNull::slice_from_raw_parts(p, size))
        }
    }
}