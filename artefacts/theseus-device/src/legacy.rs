use bcm2835_lpa::{SYSTMR, UART1};
use crate::fmt::UartWrite;
use crate::{__theseus_prog_end__, _relocation_stub, _relocation_stub_end, boot_umsg, data_synchronization_barrier, uart1};
use core::fmt::Write;

const GET_CODE : u32 = theseus_common::su_boot::Command::GetCode as u32;
const BOOT_SUCCESS : u32 = theseus_common::su_boot::Command::BootSuccess as u32;
const BOOT_ERROR : u32 = theseus_common::su_boot::Command::BootError as u32;

pub(crate) fn perform_download(uw: &mut UartWrite, uart: &UART1, _st: &SYSTMR) {
    // okay, so we just received PUT_PROGRAM_INFO
    let addr = uart1::uart1_read32_blocking(uart);
    let len = uart1::uart1_read32_blocking(uart);
    let crc = uart1::uart1_read32_blocking(uart);

    boot_umsg!(uw, "[theseus-device]: host is not THESEUS-compatible; switching to legacy SU-BOOT compatibility mode.");
    boot_umsg!(uw, "[theseus-device]: received PUT_PROGRAM_INFO: addr={addr:#010x} len={len} crc32={crc:#010x}");

    // TODO: where exactly does the stack start again???
    // stack starts at 0x8000 and goes downwards, so assume [0..&__theseus_prog_end__] is all theseus-device
    let self_end = unsafe { core::ptr::addr_of!(__theseus_prog_end__) } as usize as u32;

    let prog_begin = addr;
    let prog_end = addr + len;
    let relocate = addr < self_end;

    let (relocate_prog_from, relocate_prog_len, relocate_prog_to) = if relocate {
        (prog_begin, (self_end - prog_begin).min(len), prog_end.max(self_end))
    } else {
        (0, 0, 0)
    };
    let relocate_stub_to = relocate_prog_to + relocate_prog_len;

    boot_umsg!(uw, "[theseus-device]: relocation configuration:");
    boot_umsg!(uw, "\tRelocate: {}", if relocate { "yes" } else { "no "});
    if relocate {
        boot_umsg!(uw, "\tTarget: [{:#010x}..{:#010x}] to [{:#010x}..{:#010x}]",
            relocate_prog_from, relocate_prog_from+relocate_prog_len,
            relocate_prog_to, relocate_prog_to+relocate_prog_len);
        boot_umsg!(uw, "\tStub: [{:#010x}]",
            relocate_stub_to);
        boot_umsg!(uw, "\tSize: {}/{} KiB",
            (relocate_prog_len + 1023) / 1024, (len + 1023) / 1024);
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
        PutCode
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
    fn write_bytes_from_uart(
        uart: &UART1,
        n_bytes: usize,
        to_addr: *mut u8,
    ) {
        data_synchronization_barrier();
        let mut i = 0;
        while i < n_bytes {
            while !uart.stat().read().data_ready().bit_is_set() {}
            let b = uart.io().read().data().bits();
            unsafe { to_addr.offset(i as isize).write(b); }
            i += 1;
        }
        data_synchronization_barrier();
    }


    let verify_crc32 = if relocate {
        let mut crc = crc32fast::Hasher::new();
        unsafe {
            let relocate_prog_to_ptr = relocate_prog_to as usize as *mut u8;
            write_bytes_from_uart(
                uart,
                relocate_prog_len as usize,
                relocate_prog_to_ptr,
            );
            let stationary_len = (len - relocate_prog_len) as usize;
            let stationary_ptr = relocate_prog_to_ptr.offset(relocate_prog_len as isize);
            write_bytes_from_uart(
                uart,
                stationary_len,
                stationary_ptr,
            );
            crc.update(core::slice::from_raw_parts(relocate_prog_to_ptr, relocate_prog_len as usize));
            crc.update(core::slice::from_raw_parts(stationary_ptr, stationary_len));
            crc.finalize()
        }
    } else {
        unsafe {
            write_bytes_from_uart(
                uart,
                len as usize,
                addr as usize as *mut u8,
            );
            crc32fast::hash(core::slice::from_raw_parts(addr as usize as *mut u8, len as usize))
        }
    };

    let crc_ok = verify_crc32 == crc;
    boot_umsg!(uw, "[theseus-device]: received program, calculated CRC32 is {:#010x}, expected {:#010x}: {}", verify_crc32, crc, if crc_ok { "ok" } else { "mismatch" });

    if !crc_ok {
        boot_umsg!(uw, "[theseus-device]: fatal CRC mismatch, rebooting");
        uart1::uart1_write32(uart, BOOT_ERROR);

        return
    }

    unsafe {
        relocate_stub(
            RelocationParams {
                uw,
                uart,
                stub_dst: relocate_stub_to as usize as *mut u8,
                prog_dst: relocate_prog_from as usize as *mut u8,
                prog_src: relocate_prog_to as usize as *mut u8,
                prog_len: relocate_prog_len as usize,
                entry: addr as usize as *mut u8,
            }
        )
    }
}

struct RelocationParams<'a, 'b, 'c> {
    uw: &'a mut UartWrite<'b>,
    uart: &'c UART1,
    stub_dst: *mut u8,
    prog_dst: *mut u8,
    prog_src: *mut u8,
    prog_len: usize,
    entry: *mut u8,
}

unsafe fn relocate_stub(params: RelocationParams) -> ! {
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

    uart1::uart1_write32(uart, BOOT_SUCCESS);

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
