extern "C" {
    static mut __theseus_code_start__: u8;
    pub(crate) static __theseus_prog_end__: u8;

    static mut __theseus_bss_start__ : u8;
    static __theseus_bss_end__ : u8;

    pub(crate) static _relocation_stub: u8;
    pub(crate) static _relocation_stub_end: u8;
}

/// A contiguous region of (physical) memory.
#[derive(Debug, Eq, PartialEq)]
pub struct Span {
    /// Pointer to the first byte of the memory region.
    begin: *mut u8,
    /// Pointer to the byte *after* the last byte of the memory region.
    end: *const u8,
}

impl Span {
    pub fn new(
        begin: *mut u8,
        end: *const u8
    ) -> Self {
        Self { begin, end, }
    }
    pub unsafe fn as_bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.begin, self.len()) }
    }
    pub unsafe fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.begin, self.len()) }
    }
    pub fn len(&self) -> usize {
        unsafe { self.end.offset_from(self.begin) as usize }
    }
}

/// Zero out the memory where the .bss section should load to.
///
/// An unfortunate necessity as `objcopy` does not seem to produce flat binaries with a zeroed bss
/// section, despite it appearing in the original ELF files.
pub fn zero_stub_bss() {
    // SAFETY:
    //  noalloc so we're not writing over the heap; stack is-actually I'm not not precisely sure
    //  where it is-but in any case we're assured by the linker script that nothing is in the way of
    //  the .bss section; it's just that we need to zero it because it's uninitialized and may be
    //  filled with garbage.
    //  TODO(mcura) where is the stack
    unsafe {
        let start = core::ptr::addr_of_mut!(__theseus_bss_start__);
        let end = core::ptr::addr_of!(__theseus_bss_end__);

        let mut bss = Span::new(start, end);
        bss.as_bytes_mut()
            .iter_mut()
            .for_each(|b| *b = 0x00);
    }
}
