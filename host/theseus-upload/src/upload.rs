use std::io::{ErrorKind, Read};
use std::time::Duration;
use color_eyre::eyre;
use crate::args::Args;
use crate::bin_name;
use crate::find_tty::find_most_recent_tty_serial_device;

use theseus_common::{
    INITIAL_BAUD_RATE,
};
use crate::echo::echo;
use crate::hexify::hexify;
use crate::io::RW32;
use crate::tty::TTY;

pub fn protocol_begin(
    args: Args,
) -> eyre::Result<()> {
    let Args {
        address: _,
        device,
        baud: _,
        verbose: _, quiet: _, timestamps: _,
        trace: _,
        bin_file: _
    } = args.clone();

    let device_path = device
        .ok_or(())
        .or_else(|_| find_most_recent_tty_serial_device())
        .map_err(|e| eyre::eyre!("Unable to locate TTY device: {e}"))?;
    log::info!("[{}]: Using device {}", bin_name(), device_path.display());

    // let mut tty = serialport::new(
    //     device_path.to_str().unwrap(),
    //     INITIAL_BAUD_RATE
    // )
    //     .timeout(Duration::from_millis(100))
    //     // 8n1, no flow control
    //     .flow_control(FlowControl::None)
    //     .data_bits(DataBits::Eight)
    //     .parity(Parity::None)
    //     .stop_bits(StopBits::One)
    //     .open_native()
    //     .with_note(|| format!("while trying to open {} in 8n1 with no flow control", device_path.display()))?;
    let mut tty = TTY::new(device_path, INITIAL_BAUD_RATE)?;
    tty.set_timeout(Duration::from_millis(100))?;

    let mut succeeded = false;
    // for attempt_no in 1..=5 {
    for attempt_no in 1..=5 {
        log::info!("[{}]: Attempting ({attempt_no}/5) to promote protocol", bin_name());

        if super::theseus::try_promote(&args, &mut tty)? {
            succeeded = true;
            break
        }
    }
    if !succeeded {
        state_initial(&args, &mut tty);
        crate::legacy::begin(&args, &mut tty)?;
    }
    echo(&args, &mut tty)
}

/// Wait for GET_PROG_INFO, and figure out what version of THESEUS to execute or if we should switch
/// to SU-BOOT.
fn state_initial(
    _args: &Args,
    tty: &mut TTY
) {
    let mut status = 0;
    log::debug!("Waiting for GET_PROG_INFO");
    loop {
        let byte = tty.read8();
        if let Err(e) = byte {
            if e.kind() == ErrorKind::TimedOut {
                log::trace!("failed to read from tty: {}", e);
            } else {
                log::debug!("failed to read from tty: {}", e);
            }
            continue;
        }
        let byte = byte.unwrap();

        match (status, byte) {
            // note: it's little endian, so 11112222 will come over the wire as 22 22 11 11
            (0, 0x22) => status += 1,
            (1, 0x22) => status += 1,
            (2, 0x11) => status += 1,
            (3, 0x11) => status += 1,
            (0, 0xee) => status = 5,
            (5, 0xee) => status = 6,
            (6, 0xdd) => status = 7,
            (7, 0xdd) => {
                status = 0;
                let len = tty.read32_le().unwrap_or(0);
                if len > 0 {
                    let mut v = vec![0; len as usize];
                    let _ = tty.read_exact(&mut v);
                    log::info!("[ {}", hexify(&v));
                    log::info!("< {}", String::from_utf8_lossy(&v));
                }
            }
            (x, _) if x > 0 => status = 0,
            _ => {}
        }

        if status == 4 {
            break
        }
    }

    log::debug!("Found GET_PROG_INFO");

    log::warn!("Device does not support THESEUS protocol, falling back to SU-BOOT");
    log::warn!("Legacy SU-BOOT mode is INCOMPLETE and does not implement the commands: BOOT_START and has incomplete support for the commands: PRINT_STRING!");
}