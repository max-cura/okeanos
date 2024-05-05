use core::ptr;
use crate::reactor::StationaryBufferLayout;

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

#[derive(Clone, Debug)]
pub struct Relocation {
    pub base_address_ptr: *mut u8,
    pub side_buffer_ptr: *mut u8,
    pub relocate_first_n_bytes: usize,
    pub stub_entry: *mut u8,
    relocate: bool,
}

const PAGE_SIZE : usize = 0x4000;

impl Relocation {
    pub fn calculate(
        base_address: usize,
        k_length: usize,
        sbl: &StationaryBufferLayout,
    ) -> Relocation {
        let self_end_addr = sbl.__unsafe_stationary_buffers_end__ as usize;
        let k_base_address = base_address;
        let k_end_address = k_base_address + k_length;

        let needs_to_relocate = k_base_address < self_end_addr;

        let highest_used_address = self_end_addr.max(k_end_address);
        let side_buffer_begin = (highest_used_address + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        if needs_to_relocate {
            let relocation_length = k_end_address.min(self_end_addr) - k_base_address;
            // need this to be 4-byte aligned if we want to jump to it
            let stub_location = (side_buffer_begin + relocation_length + 3) & !3;
            Relocation {
                base_address_ptr: k_base_address as *mut u8,
                side_buffer_ptr: side_buffer_begin as *mut u8,
                relocate_first_n_bytes: relocation_length,
                stub_entry: stub_location as *mut u8,
                relocate: true,
            }
        } else {
            Relocation {
                base_address_ptr: ptr::null_mut(),
                side_buffer_ptr: ptr::null_mut(),
                relocate_first_n_bytes: 0,
                stub_entry: highest_used_address as *mut u8,
                relocate: false,
            }
        }
    }

    pub unsafe fn write_bytes(
        &self,
        address: *mut u8,
        bytes: &[u8]
    ) {
        let (ptr, len) = (bytes.as_ptr(), bytes.len());
        let write_ptr = if self.relocate
            && address >= self.base_address_ptr
            && address < unsafe { self.base_address_ptr.byte_offset(self.relocate_first_n_bytes as isize) }
        {
            unsafe {
                self.side_buffer_ptr.byte_offset(
                    address.byte_offset_from(self.base_address_ptr))
            }
        } else {
            address
        };
        unsafe {
            ptr::copy(ptr, write_ptr, len)
        };
    }

    pub unsafe fn verify_integrity(
        &self,
        _rz: &super::reactor::Reactor,
        _fs: &mut super::reactor::txbuf::FrameSink,
        expected_crc: u32,
        len: usize
    ) -> Integrity {
        let mut hasher = crc32fast::Hasher::new();
        // crate::print_rpc!(fs, "[device:v1]: verifying integrity (1)");
        // fs._flush_to_fifo(&rz.peri.UART1);
        if self.relocate {
            // crate::print_rpc!(fs, "[device:v1]: verifying integrity (2)");
            // fs._flush_to_fifo(&rz.peri.UART1);
            let side_buf = unsafe { core::slice::from_raw_parts(
                self.side_buffer_ptr,
                self.relocate_first_n_bytes
            ) };
            hasher.update(side_buf);
        }
        // let a = self.base_address_ptr.byte_offset(self.relocate_first_n_bytes as isize);
        // let b = len - self.relocate_first_n_bytes;
        // crate::print_rpc!(fs, "[device:v1]: verifying integrity (3) / {len}:{} / {a:#?}:{b}", self.relocate_first_n_bytes);
        // fs._flush_to_fifo(&rz.peri.UART1);
        let inplace_buf = unsafe { core::slice::from_raw_parts(
            self.base_address_ptr.byte_offset(self.relocate_first_n_bytes as isize),
            len - self.relocate_first_n_bytes
        ) };
        hasher.update(inplace_buf);
        // crate::print_rpc!(fs, "[device:v1]: verifying integrity (4)");
        // fs._flush_to_fifo(&rz.peri.UART1);

        let final_crc = hasher.finalize();

        if expected_crc == final_crc {
            Integrity::Ok
        } else {
            Integrity::CrcMismatch {
                expected: expected_crc,
                calculated: final_crc,
            }
        }
    }
}

pub enum Integrity {
    Ok,
    CrcMismatch { expected: u32, calculated: u32 },
}