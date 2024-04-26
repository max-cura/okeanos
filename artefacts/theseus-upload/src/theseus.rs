use std::io::{ErrorKind, Read, Write};
use std::process::exit;
use std::time::{Duration, Instant};
use crate::args::Args;

use color_eyre::{eyre, Result};
use color_eyre::eyre::WrapErr;
use encode::HostEncode;

use theseus_common::cobs::FeedState;
use theseus_common::theseus::handshake::{self, HandshakeMessageType};
use theseus_common::theseus::handshake::device::AllowedConfigsHelper;
use theseus_common::theseus::MessageTypeType;
use crate::bin_name;
use crate::hexify::hexify;
use crate::io::RW32;
use crate::tty::TTY;

pub mod v1;
pub mod encode;

impl HostEncode for handshake::host::Probe {}
impl HostEncode for handshake::host::UseConfig {}

static SUPPORTED_VERSIONS : &[u16] = &[1];
static SUPPORTED_BAUDS : &[u32] = &[115200, 230400, 576000, 921600];

// B1000000, B1152000, B1500000, B2000000, B2500000, B3000000, B3500000, B4000000, B460800,
// B500000, B576000, B921600,


pub fn determine_configuration(
    allowed_configs: &AllowedConfigsHelper,
    _args: &Args,
) -> Option<handshake::host::UseConfig> {
    let mut allowed_versions : Vec<u16> = allowed_configs.supported_versions.clone();
    let mut allowed_bauds : Vec<u32> = allowed_configs.supported_bauds.clone();
    allowed_versions.sort();
    allowed_bauds.sort();
    let highest_supported_version = allowed_versions.iter().copied().rev().find(is_version_supported)?;
    let highest_supported_baud = allowed_bauds.iter().copied().rev().find(is_baud_supported)?;

    Some(handshake::host::UseConfig {
        version: highest_supported_version,
        baud: highest_supported_baud,
    })
}

fn is_version_supported(&v: &u16) -> bool {
    SUPPORTED_VERSIONS.contains(&v)
}

fn is_baud_supported(&bd: &u32) -> bool {
    SUPPORTED_BAUDS.contains(&bd)
}

pub fn try_promote(args: &Args, tty: &mut TTY) -> Result<bool> {
    // no error, but handshake failed  (no response)
    let Some(config) = try_handshake(args, tty) else {
        log::info!("[{}]: Handshake failed.", bin_name());
        std::thread::sleep(Duration::from_millis(700));
        return Ok(false)
    };
    log::info!("[host]: Settled on version {} at {}Bd", config.version, config.baud);

    match config.version {
        1 => {
            v1::run(args, tty)?;
        }
        x => panic!("no such version: {x}"),
    }

    Ok(true)
}

pub fn try_handshake(args: &Args, tty: &mut TTY) -> Option<handshake::host::UseConfig> {
    if let Err(e) = send_message(handshake::host::Probe, tty) {
        log::error!("[host]: Failed to send probe: {e}");
        return None
    }

    // log::trace!("[host]: Sent probe.");

    let msg = match recv_bytes_blocking_timeout(tty, Duration::from_millis(100)) {
        Ok(Some(msg)) => msg,
        Ok(None) => {
            log::debug!("[host]: Received no Handshake/AllowedConfigs within timeout.");
            return None
        }
        Err(e) => {
            log::error!("[host]: Failed to read from serial port: {e}");
            return None
        }
    };
    let (typ, frame_data) = match postcard::take_from_bytes::<MessageTypeType>(&msg) {
        Ok(t) => t,
        Err(e) => {
            log::error!("[host]: Failed to deserialize message type: {e}");
            return None
        }
    };
    const HANDSHAKE_ALLOWED_CONFIGS : u32 = HandshakeMessageType::AllowedConfigs.to_u32();
    if typ != HANDSHAKE_ALLOWED_CONFIGS {
        log::error!("[host]: Expected Handshake/AllowedConfigs, got type={typ}");
        return None
    }
    let (ac, rem) = match postcard::take_from_bytes::<handshake::device::AllowedConfigs>(frame_data) {
        Ok(t) => (AllowedConfigsHelper::from(t.0), t.1),
        Err(e) => {
            log::error!("[host]: Failed to deserialize message: {e}");
            return None
        }
    };
    if !rem.is_empty() {
        log::error!("[host]: Bytes remaining in buffer after deserializing AllowedConfigs");
        return None
    }
    // log::trace!("[host]: AC={ac:?}");
    let Some(config) = determine_configuration(&ac, args) else {
        log::error!("[host]: No device-compatible version/baud configuration found");
        return None
    };

    if let Err(e) = send_message(config, tty) {
        log::error!("[host]: Failed to send UseConfig: {e}");
        return None
    }

    // send seqeuences of 5f 5f 5f 5f  5f 5f 5f 5f every ~16 B interval for 4096 B of time
    // until we receive 5f 5f 5f 5f  5f 5f 5f 5f of our own
    // at which point we stop sending, clear the 5f's, switch the protocol on our end
    // nvm we can totally just spin on our end until we receive something

    // let device_path = args.device.clone()
    //     .ok_or(())
    //     .or_else(|_| find_most_recent_tty_serial_device())
    //     .map_err(|e| eyre::eyre!("Unable to locate TTY device: {e}"))
    //     .unwrap();
    //
    // *tty = serialport::new(
    //     device_path.to_str().unwrap(),
    //     config.baud
    // )
    //     .timeout(Duration::from_millis(100))
    //     // 8n1, no flow control
    //     .flow_control(FlowControl::None)
    //     .data_bits(DataBits::Eight)
    //     .parity(Parity::None)
    //     .stop_bits(StopBits::One)
    //     .open_native()
    //     .with_note(|| format!("while trying to open {} in 8n1 with no flow control", device_path.display()))
    //     .unwrap();
    // log::info!("[host]: Reopened serial port");

    if let Err(e) = tty.set_baud_rate(config.baud) {
        log::error!("[host]: Failed to set baud rate: {e}");
        return None
    }

    // tty.clear(ClearBuffer::All).unwrap();

    Some(config)
}

fn send_message<T: HostEncode>(msg: T, tty: &mut TTY) -> Result<()> {
    let frame = encode::frame_bytes(&msg.encode()?)?;
    send_bytes(&frame, tty)?;
    tty.flush()?;
    Ok(())
}

fn send_bytes(frame: &[u8], tty: &mut TTY) -> Result<()> {
    log::trace!("[host] > {}", hexify(frame));

    tty.write_all(frame)
        .map_err(|e| eyre::eyre!("failed to send: {e}"))?;
    tty.flush()?;
    Ok(())
}

// PRINT_STRING: LEGACY
pub fn recv_bytes_blocking_timeout(tty: &mut TTY, timeout: Duration) -> Result<Option<Vec<u8>>> {
    {
        let start = Instant::now();
        let mut state = 0;
        // log::trace!("[host] begin blocking receive");
        loop {
            if start.elapsed() > timeout {
                log::trace!("[host] receive timeout");
                return Ok(None)
            }
            let byte = match tty.read8() {
                Ok(b) => b,
                Err(e) if e.kind() == ErrorKind::TimedOut => {
                    log::trace!("[host] read8 timeout");
                    continue
                    // return Ok(None)
                }
                Err(e) if e.kind() == ErrorKind::BrokenPipe => {
                    log::error!("[{}]: Device disconnected. Aborting.", bin_name());
                    exit(1);
                }
                e @ Err(_) => e?
            };
            // log::trace!("[host] BYTE < {byte:#04x}");
            state = match (state, byte) {
                (0, 0x55) => 1,
                (1, 0x55) => 2,
                (2, 0x55) => 3,
                (3, 0x5e) => break,
                (3, 0x55) => 3,

                (0, 0xee) => 5,
                (5, 0xee) => 6,
                (6, 0xdd) => 7,
                (7, 0xdd) => {
                    let len = tty.read32_le().unwrap_or(0);
                    if len > 0 {
                        let mut v = vec![0; len as usize];
                        let _ = tty.read_exact(&mut v).inspect_err(|e| log::error!("PRINT_STRING read_exact failed: {e}"));
                        // log::trace!("< {}", hexify(&v));
                        log::info!("< {}", String::from_utf8_lossy(&v));
                    }
                    0
                }

                _ => 0,
            };
        }
    }

    let len = {
        let mut bytes = [0;4];
        for byte_no in 0..4 {
            bytes[byte_no] = tty.read8()
                .with_context(|| if byte_no == 0 { "while waiting for FRAME.LEN" } else { "while reading FRAME.LEN" })?;
        }
        let len = theseus_common::theseus::len::decode_len(&bytes);
        if len < 4 {
            eyre::bail!("expected frame length, got len<4, bytes: {bytes:?}");
        }
        len as usize
    };

    let cobs_frame = {
        let mut cobs_frame = vec![0; len];
        tty.read_exact(cobs_frame.as_mut())
            .with_context(|| "while waiting for FRAME.COBS_FRAME")?;
        // undo XOR55
        cobs_frame.iter_mut().for_each(|b| *b = *b ^ 0x55);

        // COBS DECODE
        let mut line_decoder = theseus_common::cobs::LineDecoder::new();
        let mut cooked = vec![];
        for b in cobs_frame {
            match line_decoder.feed(b) {
                FeedState::PacketFinished => {}
                FeedState::Byte(b) => { cooked.push(b) }
                FeedState::Pass => {}
            }
        }
        cooked
    };

    // log::trace!("frame: {}", hexify(&cobs_frame));

    let cobs_len = cobs_frame.len();

    // check CRC
    let message_crc = {
        let crc_bytes = &cobs_frame[(cobs_len - 4)..];
        u32::from_le_bytes([
            crc_bytes[0],
            crc_bytes[1],
            crc_bytes[2],
            crc_bytes[3]])
    };
    let calc_crc = crc32fast::hash(&cobs_frame[..(cobs_len-4)]);

    if message_crc == calc_crc {
        Ok(Some(cobs_frame[..(cobs_len-4)].to_vec()))
    } else {
        eyre::bail!("CRC mismatch: message had checksum {message_crc}, host computed {calc_crc}, COBS frame: [{}]", hexify(&cobs_frame));
    }
}
