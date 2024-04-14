use std::io;
use std::io::Write;
use std::sync::mpsc::TryRecvError;
use std::sync::{Mutex, OnceLock};
use color_eyre::eyre;
use serialport::TTYPort;
use crate::args::Args;
use crate::bin_name;
use crate::io::RW32;

pub fn echo(_args: &Args, tty: &mut TTYPort) -> eyre::Result<()> {
    static ETERNAL_STDIN : OnceLock<Mutex<std::sync::mpsc::Receiver<String>>> = OnceLock::new();
    let rx = ETERNAL_STDIN.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in io::stdin().lines() {
                if line.is_err() {
                    log::error!("[{}]: failed to read from stdin: {}", bin_name(), line.unwrap_err());
                } else {
                    let line = line.unwrap();
                    tx.send(line).unwrap();
                }
            }
        });
        Mutex::new(rx)
    });
    loop {
        let b = tty.read8();
        if b.is_ok() {
            io::stdout().write_all(&[b.unwrap(),])?;
        }
        match rx.lock().unwrap().try_recv() {
            Ok(s) => {
                match tty.write_all(s.as_bytes()) {
                    Ok(_) => {
                        log::trace!("> wrote successfully")
                    }
                    Err(e) => {
                        log::error!("[{}]: failed to send: {}", bin_name(), e)
                    }
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                log::error!("[{}]: ETERNAL_STDIN disconnected", bin_name());
            }
        }
    }
}

