use std::io;
use std::io::{ErrorKind, Write};
use std::sync::mpsc::TryRecvError;
use std::sync::{Mutex, OnceLock};
use color_eyre::eyre;
use crate::args::Args;
use crate::bin_name;
use crate::io::RW32;
use crate::tty::TTY;

pub fn echo(_args: &Args, tty: &mut TTY) -> eyre::Result<()> {
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
        match tty.read8() {
            Ok(b) => io::stdout().write_all(&[b,])?,
            Err(e) => {
                if e.kind() != ErrorKind::TimedOut {
                    log::error!("[{}]: error on {}: {}", bin_name(), tty.path().file_name().unwrap_or(tty.path().as_os_str()).to_string_lossy(), e);
                    std::process::exit(1);
                }
            }
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

