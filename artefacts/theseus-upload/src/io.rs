use std::io;
use std::io::ErrorKind;
use std::process::exit;
use serialport::TTYPort;
use crate::bin_name;

fn pipe_error(e: &io::Error) {
    if e.kind() == ErrorKind::BrokenPipe {
        log::error!("[{}]: Device disconnected. Aborting.", bin_name());
        exit(1);
    }
}

pub trait RW32: io::Write + io::Read {
    fn write32_le(&mut self, w: u32) -> io::Result<()> {
        self.write_all(&u32::to_le_bytes(w))
    }

    fn read32_le(&mut self) -> io::Result<u32> {
        let mut buf = [0; 4];
        self.read_exact(&mut buf).inspect_err(pipe_error)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read8(&mut self) -> io::Result<u8> {
        let mut buf : [u8; 1] = [0];
        self.read_exact(&mut buf).inspect_err(pipe_error)?;
        Ok(buf[0])
    }
}

impl RW32 for TTYPort {}
