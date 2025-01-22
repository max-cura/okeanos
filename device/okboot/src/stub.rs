unsafe extern "C" {
    static __symbol_exec_start__: [u8; 0];
    static __symbol_code_start__: [u8; 0];
    static __symbol_code_end__: [u8; 0];
    static __symbol_rodata_start__: [u8; 0];
    static __symbol_rodata_end__: [u8; 0];
    static __symbol_data_start__: [u8; 0];
    static __symbol_data_end__: [u8; 0];
    pub static __symbol_bss_start__: [u8; 0];
    pub static __symbol_bss_end__: [u8; 0];
    pub static __symbol_exec_end__: [u8; 0];
}

unsafe extern "C" {
    pub(crate) static __symbol_relocation_stub: [u8; 0];
    pub(crate) static __symbol_relocation_stub_end: [u8; 0];
}

pub unsafe fn locate_end() -> *const [u8; 0] {
    &raw const __symbol_exec_end__
}

pub mod flat_binary {
    use crate::arch::arm1176::PAGE_SIZE;
    use crate::buf::FrameSink;
    use crate::legacy_print_string_blocking;
    use crate::stub::{__symbol_relocation_stub, __symbol_relocation_stub_end};
    use bcm2835_lpa::Peripherals;

    #[derive(Clone, Debug)]
    pub struct Relocation {
        pub base_address_ptr: *mut u8,
        pub side_buffer_ptr: *mut u8,
        pub relocate_first_n_bytes: usize,
        pub stub_entry: *mut u8,
        relocate: bool,
    }

    impl Relocation {
        pub fn calculate(base_address: usize, k_length: usize, self_end_addr: usize) -> Relocation {
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
                    base_address_ptr: core::ptr::null_mut(),
                    side_buffer_ptr: core::ptr::null_mut(),
                    relocate_first_n_bytes: 0,
                    stub_entry: highest_used_address as *mut u8,
                    relocate: false,
                }
            }
        }

        pub unsafe fn write_bytes(&self, address: *mut u8, bytes: &[u8]) {
            let (ptr, len) = (bytes.as_ptr(), bytes.len());
            let write_ptr = if self.relocate
                && address >= self.base_address_ptr
                && address
                    < unsafe {
                        self.base_address_ptr
                            .byte_offset(self.relocate_first_n_bytes as isize)
                    } {
                unsafe {
                    self.side_buffer_ptr
                        .byte_offset(address.byte_offset_from(self.base_address_ptr))
                }
            } else {
                address
            };
            unsafe { core::ptr::copy(ptr, write_ptr, len) };
        }

        pub unsafe fn verify_integrity(&self, expected_crc: u32, len: usize) -> Integrity {
            // let peri = unsafe { Peripherals::steal() };
            let mut hasher = crc32fast::Hasher::new();
            // crate::print_rpc!(fs, "[device:v1]: verifying integrity (1)");
            // fs._flush_to_fifo(&rz.peri.UART1);
            if self.relocate {
                // crate::print_rpc!(fs, "[device:v1]: verifying integrity (2)");
                // fs._flush_to_fifo(&rz.peri.UART1);
                let side_buf = unsafe {
                    core::slice::from_raw_parts(self.side_buffer_ptr, self.relocate_first_n_bytes)
                };
                // legacy_print_string_blocking!(&peri.UART1, "[v2/rel] crc buf {:02x?}", side_buf,);
                hasher.update(side_buf);
            }
            // let a = self.base_address_ptr.byte_offset(self.relocate_first_n_bytes as isize);
            // let b = len - self.relocate_first_n_bytes;
            // crate::print_rpc!(fs, "[device:v1]: verifying integrity (3) / {len}:{} / {a:#?}:{b}", self.relocate_first_n_bytes);
            // fs._flush_to_fifo(&rz.peri.UART1);
            if len > self.relocate_first_n_bytes {
                let inplace_buf = unsafe {
                    core::slice::from_raw_parts(
                        self.base_address_ptr
                            .byte_offset(self.relocate_first_n_bytes as isize),
                        len - self.relocate_first_n_bytes,
                    )
                };
                // legacy_print_string_blocking!(&peri.UART1, "[v2/rel] crc buf {:02x?}", inplace_buf,);
                hasher.update(inplace_buf);
            }
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

    pub unsafe fn final_relocation(
        peripherals: &Peripherals,
        fs: &mut FrameSink,
        relocation: Relocation,
    ) -> ! {
        let stub_dst = relocation.stub_entry;
        let kernel_dst = relocation.base_address_ptr;
        let kernel_src = relocation.side_buffer_ptr;
        let kernel_copy_len = relocation.relocate_first_n_bytes;
        let kernel_entry = relocation.base_address_ptr;

        let stub_begin = &raw const __symbol_relocation_stub;
        let stub_end = &raw const __symbol_relocation_stub_end;

        let stub_len = stub_end.byte_offset_from(stub_begin) as usize;

        crate::legacy_print_string_blocking!(
            &peripherals.UART1,
            "[device:v1]: relocation_stub parameters:"
        );
        crate::legacy_print_string_blocking!(
            &peripherals.UART1,
            "\tstub destination={stub_dst:#?}"
        );
        crate::legacy_print_string_blocking!(&peripherals.UART1, "\tstub code={stub_begin:#?}");
        crate::legacy_print_string_blocking!(&peripherals.UART1, "\tstub length={stub_len:#?}");
        crate::legacy_print_string_blocking!(&peripherals.UART1, "\tcopy to={kernel_dst:#?}");
        crate::legacy_print_string_blocking!(&peripherals.UART1, "\tcopy from={kernel_src:#?}");
        crate::legacy_print_string_blocking!(&peripherals.UART1, "\tcopy bytes={kernel_copy_len}");
        crate::legacy_print_string_blocking!(&peripherals.UART1, "\tentry={kernel_entry:#?}");

        core::ptr::copy(stub_begin as *const u8, stub_dst, stub_len);

        crate::legacy_print_string_blocking!(
            &peripherals.UART1,
            "[device:v1]: Loaded relocation-stub, jumping"
        );

        crate::protocol::flush_to_fifo(fs, &peripherals.UART1);
        crate::mini_uart::mini_uart1_flush_tx(&peripherals.UART1);

        core::arch::asm!(
            "bx {t0}",
            in("r0") kernel_dst,
            in("r1") kernel_src,
            in("r2") kernel_copy_len,
            in("r3") kernel_entry,
            t0 = in(reg) stub_dst,
            options(noreturn),
        )
    }
}
