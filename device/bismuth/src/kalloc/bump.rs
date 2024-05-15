use core::alloc::Layout;
use core::ptr::NonNull;
use crate::kalloc::PAGE_SIZE;

#[derive(Debug, Copy, Clone)]
pub struct BumpAllocator {
    begin: *mut u8,
    end: *mut u8,
    current: *mut u8,
}

impl BumpAllocator {
    pub fn new(begin: *mut u8, end: *mut u8) -> Self {
        Self {
            begin,
            end,
            current: begin,
        }
    }
    fn round_to_page(layout: Layout) -> Option<Layout> {
        Some(layout.align_to(PAGE_SIZE).ok()?.pad_to_align())
    }
    pub fn allocate_pages(&mut self, layout: Layout) -> Option<NonNull<[u8]>> {
        let layout = Self::round_to_page(layout)?;
        let alloc_begin = unsafe { self.current.offset(self.current.align_offset(PAGE_SIZE) as isize) };
        let alloc_end = unsafe { alloc_begin.byte_offset(layout.size() as isize) };
        if alloc_end >= self.end { return None }
        Some(NonNull::slice_from_raw_parts(
            NonNull::new(alloc_begin)?,
            unsafe { alloc_end.byte_offset_from(alloc_begin) as usize },
        ))
    }
    pub fn allocate(&mut self, layout: Layout) -> Option<NonNull<[u8]>> {
        let alloc_begin = unsafe { self.current.offset(self.current.align_offset(layout.align()) as isize) };
        let alloc_end = unsafe { alloc_begin.byte_offset(layout.size() as isize) };
        if alloc_end >= self.end { return None }
        Some(NonNull::slice_from_raw_parts(
            NonNull::new(alloc_begin)?,
            unsafe { alloc_end.byte_offset_from(alloc_begin) as usize },
        ))
    }
    pub fn consume(self) -> (*mut u8, *mut u8) {
        (self.current, self.end)
    }
}

