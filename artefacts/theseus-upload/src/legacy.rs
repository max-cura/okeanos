use std::io::{self, Write};
use std::process::exit;
use color_eyre::{eyre, Section};
use serialport::{ClearBuffer, SerialPort, TTYPort};
use theseus_common::su_boot::Command;
use theseus_common::theseus::ProgramCRC32;
use crate::args::{Args, TraceLevel};
use crate::bin_name;
use crate::io::RW32;


struct Write32<'a> {
    inner: &'a mut TTYPort,
    trace_control: bool,
    trace_data: bool,
}
impl<'a> Write32<'a> {
    pub fn new(inner: &'a mut TTYPort, tl: TraceLevel) -> Self {
        Self {
            inner,
            trace_control: tl != TraceLevel::Off,
            trace_data: tl == TraceLevel::All,
        }
    }
    fn write32_le(&mut self, w: u32, ctrl: bool) -> io::Result<()> {
        if self.trace_data || (self.trace_control && ctrl) {
            log::trace!("> writing {w:#010x}");
        }
        self.inner.write32_le(w)
    }
}

fn with_write32<'a>(
    tty: &'a mut TTYPort,
    tl: TraceLevel,
    f: impl FnOnce(Write32<'a>) -> io::Result<()>
) -> io::Result<()> {
    f(Write32::new(tty, tl))
}

pub(crate) fn begin(args: &Args, tty: &mut TTYPort) -> eyre::Result<()> {
    log::info!("[{}]: Using legacy SU-BOOT protocol", bin_name());

    if args.trace == TraceLevel::All {
        log::warn!("Legacy SU-BOOT mode does not support trace=all");
    }

    log::debug!("clearing buffers");
    tty.clear(ClearBuffer::All)?;

    let prog_data = std::fs::read(args.bin_file.as_path())?;

    let mut crc = ProgramCRC32::new();
    crc.add_data(&prog_data);
    let crc32 = crc.finalize();

    // PUT_PROG_INFO

    log::debug!("Writing PUT_PROG_INFO");

    with_write32(tty, args.trace, |mut w| {
        w.write32_le(Command::PutProgInfo as u32, true)?;
        w.write32_le(args.address, true)?;
        w.write32_le(prog_data.len().try_into().unwrap(), true)?;
        w.write32_le(crc32, true)?;
        Ok(())
    })?;

    tty.flush()?;

    let mut status = 0;
    let mut switch = 0;
    loop {
        let byte = tty.read8();
        if byte.is_err() {
            log::debug!("failed to read from tty: {}", byte.unwrap_err());
            continue;
        }
        let byte = byte.unwrap();
        // BOOT_ERROR (bbbbcccc) or GET_CODE (55556666)
        match (status, switch, byte) {
            (0, 0, 0xcc) => { switch = 1; status += 1 }
            (1, 1, 0xcc) => { status += 1 }
            (2, 1, 0xbb) => { status += 1 }
            (3, 1, 0xbb) => { status += 1 }
            (0, 0, 0x66) => { switch = 2; status += 1 }
            (1, 2, 0x66) => { status += 1 }
            (2, 2, 0x55) => { status += 1 }
            (3, 2, 0x55) => { status += 1 }
            (0, 0, _) => {},
            _ => { switch = 0; status = 0 }
        }
        if status == 4 { break }
    }

    if switch == 1 {
        // BOOT_ERROR
        log::error!("Current settings would lead to a code collision on the device! Aborting.");
        exit(1);
    } else if switch == 2 {
        // GET_CODE
        log::debug!("Received GET_CODE");
    } else {
        unreachable!("state machine has two end states, 1 and 2; got neither");
    }

    let retransmitted_crc = tty.read32_le().with_note(|| "while reading retransmitted CRC")?;

    if retransmitted_crc != crc32 {
        log::error!("Bad CRC: sent {crc32:#010x}, received {retransmitted_crc:#010x}! Aborting.");
        exit(1);
    }
    log::info!("[{}]: Received correct CRC, sending data", bin_name());

    // PUT_CODE

    log::debug!("Writing PUT_CODE");
    with_write32(tty, args.trace, |mut w| {
        w.write32_le(Command::PutCode as u32, true)
    })?;
    log::debug!("Writing data");
    tty.write_all(&prog_data)?;
    log::info!("[{}]: Finished writing data", bin_name());

    // wait for BOOT_START ?, BOOT_SUCCESS, BOOT_ERROR

    let mut status = 0;
    let mut switch = 0;
    loop {
        let byte = tty.read8();
        if byte.is_err() {
            log::debug!("failed to read from tty: {}", byte.unwrap_err());
            continue;
        }
        let byte = byte.unwrap();
        // BOOT_ERROR (bbbbcccc) or BOOT_SUCCESS (9999aaaa)
        match (status, switch, byte) {
            (0, 0, 0xcc) => { switch = 1; status += 1 }
            (1, 1, 0xcc) => { status += 1 }
            (2, 1, 0xbb) => { status += 1 }
            (3, 1, 0xbb) => { status += 1 }
            (0, 0, 0xaa) => { switch = 2; status += 1 }
            (1, 2, 0xaa) => { status += 1 }
            (2, 2, 0x99) => { status += 1 }
            (3, 2, 0x99) => { status += 1 }
            (0, 0, _) => {},
            _ => { switch = 0; status = 0 }
        }
        if status == 4 { break }
    }
    if switch == 1 {
        // BOOT_ERROR
        log::error!("Current settings would lead to a code collision on the device! Aborting.");
        exit(1);
    } else if switch == 2 {
        // BOOT_SUCCESS
        log::info!("[{}]: Device booted successfully.", bin_name());
    } else {
        unreachable!("state machine has two end states, 1 and 2; got neither");
    }
    Ok(())
}