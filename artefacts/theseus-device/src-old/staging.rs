use crate::{__theseus_prog_end__, _relocation_stub, _relocation_stub_end, boot_umsg, uart1};
use crate::fmt::UartWrite;
use core::fmt::Write;
use bcm2835_lpa::UART1;

#[derive(Debug, Copy, Clone)]
pub struct RelocationConfig {
    pub desired_location: usize,
    pub side_buffer_location: usize,
    pub relocate_first_n_bytes: usize,
    pub stub_location: usize,
}

impl RelocationConfig {
    pub fn new() -> Self {
        Self {
            desired_location: 0,
            side_buffer_location: 0,
            relocate_first_n_bytes: 0,
            stub_location: 0,
        }
    }
}

const PAGE_SIZE : usize = 0x4000;

pub fn calculate(
    load_at_addr: usize,
    length: usize,
) -> RelocationConfig {
    let self_end_addr = unsafe { core::ptr::addr_of!(__theseus_prog_end__)} as usize;

    let loaded_program_begin = load_at_addr;
    let loaded_program_end = load_at_addr + length;
    let needs_to_relocate = load_at_addr < self_end_addr;

    let highest_address = self_end_addr.max(loaded_program_end);

    let side_buffer_begin = (highest_address + PAGE_SIZE - 1) & !(PAGE_SIZE-1);

    if needs_to_relocate {
        let relocation_length = loaded_program_end.min(self_end_addr) - loaded_program_begin;

        let stub_location = (side_buffer_begin + relocation_length + 3) & !3;

        RelocationConfig {
            desired_location: load_at_addr,
            side_buffer_location: side_buffer_begin,
            relocate_first_n_bytes: relocation_length,
            stub_location
        }
    } else {
        RelocationConfig {
            desired_location: 0,
            side_buffer_location: 0,
            relocate_first_n_bytes: 0,
            stub_location: highest_address
        }
    }
}

pub fn write_bytes_with_relocation(
    relocation_config: &RelocationConfig,
    address: usize,
    bytes: &[u8],
) {
    let (ptr, len) = (bytes.as_ptr(), bytes.len());
    let write_ptr = if relocation_config.relocate_first_n_bytes > 0
        && address >= relocation_config.desired_location
        && address < (relocation_config.desired_location + relocation_config.relocate_first_n_bytes)
    {
        // okay, relocate
        let side_address
            = (address - relocation_config.desired_location)
            + relocation_config.side_buffer_location;
        side_address
    } else {
        address
    } as *mut u8;

    unsafe { core::ptr::copy(ptr, write_ptr, len) }
}

pub enum Integrity {
    Ok,
    CrcMismatch {
        expected: u32,
        got: u32,
    }
}

pub(crate) fn verify_integrity(
    uw: &mut UartWrite,
    relocation_config: &RelocationConfig,
    crc: u32,
    len: usize,
) -> Integrity {
    let mut hasher = crc32fast::Hasher::new();

    if relocation_config.relocate_first_n_bytes > 0 {
        boot_umsg!(uw, "RFNB: checking {:#010x}:{}", relocation_config.side_buffer_location, relocation_config.relocate_first_n_bytes);
        let side_buf = unsafe { core::slice::from_raw_parts(
            relocation_config.side_buffer_location as *const u8,
            relocation_config.relocate_first_n_bytes
        ) };
        hasher.update(side_buf);
    }
    boot_umsg!(uw, "checking {:#010x}:{}",
        relocation_config.desired_location + relocation_config.relocate_first_n_bytes,
        len - relocation_config.relocate_first_n_bytes,
    );
    let inplace_buf = unsafe {
        core::slice::from_raw_parts(
            (relocation_config.desired_location + relocation_config.relocate_first_n_bytes) as *const u8,
            len - relocation_config.relocate_first_n_bytes,
        )
    };
    hasher.update(inplace_buf);

    let final_crc = hasher.finalize();

    if crc == final_crc { Integrity::Ok } else { Integrity::CrcMismatch {
        expected: crc,
        got: final_crc,
    }}
}

pub struct RelocationParams<'a, 'b, 'c> {
    pub uw: &'a mut UartWrite<'b>,
    pub uart: &'c UART1,
    pub stub_dst: *mut u8,
    pub prog_dst: *mut u8,
    pub prog_src: *mut u8,
    pub prog_len: usize,
    pub entry: *mut u8,
}

pub unsafe fn relocate_stub_inner<F: FnOnce(&UART1)>(
    params: RelocationParams,
    f: F
) -> ! {
    let RelocationParams {
        uw, uart, stub_dst, prog_dst, prog_src, prog_len, entry
    } = params;
    let stub_begin = core::ptr::addr_of!(_relocation_stub);
    let stub_end = core::ptr::addr_of!(_relocation_stub_end);

    let stub_len = stub_end.offset_from(stub_begin) as usize;

    boot_umsg!(uw, "[theseus-device]: relocation_stub parameters:");
    boot_umsg!(uw, "\tstub_dst={stub_dst:#?}");
    boot_umsg!(uw, "\tstub_loc={stub_begin:#?}");
    boot_umsg!(uw, "\tstub_len={stub_len} bytes");
    boot_umsg!(uw, "\tprog_dst={prog_dst:#?}");
    boot_umsg!(uw, "\tprog_src={prog_src:#?}");
    boot_umsg!(uw, "\tprog_len={prog_len} bytes");
    boot_umsg!(uw, "\tentry={entry:#?}");

    core::ptr::copy(
        stub_begin as *const u8,
        stub_dst,
        stub_len
    );

    boot_umsg!(uw, "[theseus-device]: loaded relocation-stub, jumping to relocated stub.");

    uart1::uart1_flush_tx(uart);

    f(uart);

    // uart1::uart1_write32(uart, BOOT_SUCCESS);

    uart1::uart1_flush_tx(uart);

    core::arch::asm!(
        // "sev",
        "bx {t0}",
        in("r0") prog_dst,
        in("r1") prog_src,
        in("r2") prog_len,
        in("r3") entry,
        t0 = in(reg) stub_dst,
        options(noreturn),
    )

    // boot_umsg!(uw, "[theseus-device]: ... well we should have jumped into the stub; I'm not really sure what just happened.");
    // boot_umsg!(uw, "[theseus-device]: bad program state, entering infinite loop");
    //
    // // unreachable
    // loop {}
}
