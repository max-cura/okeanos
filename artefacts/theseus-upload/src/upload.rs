use std::time::Duration;
use color_eyre::{eyre, Section as _};
use serialport::{DataBits, FlowControl, Parity, StopBits, TTYPort};
use crate::args::Args;
use crate::bin_name;
use crate::find_tty::find_most_recent_tty_serial_device;

use theseus_common::{
    INITIAL_BAUD_RATE,
};
use theseus_common::theseus::{TheseusVersion, validate_version, VersionValidation};
use crate::echo::echo;
use crate::io::RW32;

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

    let mut tty = serialport::new(
        device_path.to_str().unwrap(),
        INITIAL_BAUD_RATE
    )
        .timeout(Duration::from_millis(100))
        // 8n1, no flow control
        .flow_control(FlowControl::None)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .open_native()
        .with_note(|| format!("while trying to open {} in 8n1 with no flow control", device_path.display()))?;

    match state_initial(&args, &mut tty) {
        InitialBranch::Legacy => {
            crate::legacy::begin(&args, &mut tty)?;
            echo(&args, &mut tty)
        }
        InitialBranch::Theseus { version } => {
            crate::theseus::version_dispatch(version, &args, &mut tty)?;
            echo(&args, &mut tty)
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum InitialBranch {
    Legacy,
    Theseus { version: TheseusVersion },
}

/// Wait for GET_PROG_INFO, and figure out what version of THESEUS to execute or if we should switch
/// to SU-BOOT.
fn state_initial(
    _args: &Args,
    tty: &mut TTYPort
) -> InitialBranch {
    let mut status = 0;
    log::debug!("Waiting for GET_PROG_INFO");
    loop {
        let byte = tty.read8();
        if byte.is_err() {
            log::debug!("failed to read from tty: {}", byte.unwrap_err());
            continue;
        }
        let byte = byte.unwrap();

        match (status, byte) {
            // note: it's little endian, so 11112222 will come over the wire as 22 22 11 11
            (0, 0x22) => status += 1,
            (1, 0x22) => status += 1,
            (2, 0x11) => status += 1,
            (3, 0x11) => status += 1,
            (x, _) if x > 0 => status = 0,
            _ => {}
        }

        if status == 4 {
            break
        }
    }

    log::debug!("Found GET_PROG_INFO");

    use std::io::Read as _;
    let mut version_word1_buf : [u8 ; 4] = [0,0,0,0];
    let _ = tty.read_exact(&mut version_word1_buf);
    let version_word1 = u32::from_le_bytes(version_word1_buf);

    log::debug!("THESEUS version word: {version_word1:#010x}");

    match validate_version(version_word1) {
        Ok(version) => InitialBranch::Theseus {version},
        Err(e) => {
            match e {
                VersionValidation::ValidUnknown => {
                    log::warn!("Device supports THESEUS protocol version that {} does not know, falling back to {}", bin_name(), TheseusVersion::max_value());
                    InitialBranch::Theseus { version: TheseusVersion::max_value() }
                }
                VersionValidation::Invalid => {
                    log::warn!("Device does not support THESEUS protocol, falling back to SU-BOOT");
                    log::warn!("Note that legacy SU-BOOT mode is INCOMPLETE and does not implement the commands: PRINT_STRING BOOT_START!");
                    InitialBranch::Legacy
                }
            }
        }
    }
}