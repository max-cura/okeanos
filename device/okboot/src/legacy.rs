pub(crate) mod fmt;
mod staging;
pub mod uart1;

use crate::arch::arm1176::__dsb;
use crate::legacy_print_string_blocking;
use bcm2835_lpa::UART1;

const GET_CODE: u32 = okboot_common::su_boot::Command::GetCode as u32;
const BOOT_SUCCESS: u32 = okboot_common::su_boot::Command::BootSuccess as u32;
const BOOT_ERROR: u32 = okboot_common::su_boot::Command::BootError as u32;

pub fn perform_download(uart: &UART1) {
    // okay, so we just received PUT_PROGRAM_INFO
    let addr = uart1::uart1_read32_blocking(uart);
    let len = uart1::uart1_read32_blocking(uart);
    let crc = uart1::uart1_read32_blocking(uart);

    legacy_print_string_blocking!(uart, "[theseus-device]: host is not THESEUS-compatible; switching to legacy SU-BOOT compatibility mode.");
    legacy_print_string_blocking!(uart, "[theseus-device]: received PUT_PROGRAM_INFO: addr={addr:#010x} len={len} crc32={crc:#010x}");

    // TODO: where exactly does the stack start again???
    // stack starts at 0x8000 and goes downwards, so assume [0..&__theseus_prog_end__] is all theseus-device
    let self_end = (&raw const crate::stub::__symbol_exec_end__).addr() as u32;

    let prog_begin = addr;
    let prog_end = addr + len;
    let relocate = addr < self_end;

    let (relocate_prog_from, relocate_prog_len, relocate_prog_to) = if relocate {
        (
            prog_begin,
            (self_end - prog_begin).min(len),
            prog_end.max(self_end),
        )
    } else {
        (0, 0, 0)
    };
    let relocate_stub_to = relocate_prog_to + relocate_prog_len;

    legacy_print_string_blocking!(uart, "[theseus-device]: relocation configuration:");
    legacy_print_string_blocking!(uart, "\tRelocate: {}", if relocate { "yes" } else { "no " });
    if relocate {
        legacy_print_string_blocking!(
            uart,
            "\tTarget: [{:#010x}..{:#010x}] to [{:#010x}..{:#010x}]",
            relocate_prog_from,
            relocate_prog_from + relocate_prog_len,
            relocate_prog_to,
            relocate_prog_to + relocate_prog_len
        );
        legacy_print_string_blocking!(uart, "\tStub: [{:#010x}]", relocate_stub_to);
        legacy_print_string_blocking!(
            uart,
            "\tSize: {}/{} KiB",
            (relocate_prog_len + 1023) / 1024,
            (len + 1023) / 1024
        );
    }

    // need to respond with GET_CODE - BOOT_ERROR doesn't apply since we will relocate ourselves
    uart1::uart1_write32(uart, GET_CODE);
    // CRC verification
    uart1::uart1_write32(uart, crc);

    enum S {
        CLR,
        PC1,
        PC2,
        PC3,
        PutCode,
    }

    let mut state = S::CLR;

    // wait for PUT_CODE
    loop {
        let Some(byte) = uart1::uart1_read8_nb(uart) else {
            continue;
        };

        state = match (state, byte) {
            (S::CLR, 0x88) => S::PC1,
            (S::PC1, 0x88) => S::PC2,
            (S::PC2, 0x77) => S::PC3,
            (S::PC3, 0x77) => S::PutCode,
            _ => S::CLR,
        };
        if matches!(state, S::PutCode) {
            break;
        }
    }

    #[no_mangle]
    #[inline(never)]
    fn write_bytes_from_uart(uart: &UART1, n_bytes: usize, to_addr: *mut u8) {
        __dsb();
        let mut i = 0;
        while i < n_bytes {
            while !uart.stat().read().data_ready().bit_is_set() {}
            let b = uart.io().read().data().bits();
            unsafe {
                to_addr.offset(i as isize).write(b);
            }
            i += 1;
        }
        __dsb();
    }

    let verify_crc32 = if relocate {
        let mut crc = crc32fast::Hasher::new();
        unsafe {
            let relocate_prog_to_ptr = relocate_prog_to as usize as *mut u8;
            write_bytes_from_uart(uart, relocate_prog_len as usize, relocate_prog_to_ptr);
            let stationary_len = (len - relocate_prog_len) as usize;
            let stationary_ptr = relocate_prog_to_ptr.offset(relocate_prog_len as isize);
            write_bytes_from_uart(uart, stationary_len, stationary_ptr);
            crc.update(core::slice::from_raw_parts(
                relocate_prog_to_ptr,
                relocate_prog_len as usize,
            ));
            crc.update(core::slice::from_raw_parts(stationary_ptr, stationary_len));
            crc.finalize()
        }
    } else {
        unsafe {
            write_bytes_from_uart(uart, len as usize, addr as usize as *mut u8);
            crc32fast::hash(core::slice::from_raw_parts(
                addr as usize as *mut u8,
                len as usize,
            ))
        }
    };

    let crc_ok = verify_crc32 == crc;
    legacy_print_string_blocking!(
        uart,
        "[theseus-device]: received program, calculated CRC32 is {:#010x}, expected {:#010x}: {}",
        verify_crc32,
        crc,
        if crc_ok { "ok" } else { "mismatch" }
    );

    if !crc_ok {
        legacy_print_string_blocking!(uart, "[theseus-device]: fatal CRC mismatch, rebooting");
        uart1::uart1_write32(uart, BOOT_ERROR);

        return;
    }

    unsafe {
        relocate_stub(staging::RelocationParams {
            uart,
            stub_dst: relocate_stub_to as usize as *mut u8,
            prog_dst: relocate_prog_from as usize as *mut u8,
            prog_src: relocate_prog_to as usize as *mut u8,
            prog_len: relocate_prog_len as usize,
            entry: addr as usize as *mut u8,
        })
    }
}

unsafe fn relocate_stub(params: staging::RelocationParams) -> ! {
    fn f(uart: &UART1) {
        uart1::uart1_write32(uart, BOOT_SUCCESS);
    }
    staging::relocate_stub_inner(params, f)
}
