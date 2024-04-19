use std::io::{ErrorKind, Read, Write};
use std::process::exit;
use std::time::{Duration, Instant};
use serialport::TTYPort;
use crate::args::Args;

use color_eyre::{Result, eyre};
use color_eyre::eyre::WrapErr;

use theseus_common::cobs::{BufferedEncoder, EncodeState};
use theseus_common::theseus as protocol;
use theseus_common::theseus::handshake::{self, HandshakeMessageType};
use theseus_common::theseus::MessageTypeType;
use crate::bin_name;
use crate::io::RW32;

static SUPPORTED_VERSIONS : &[u16] = &[0];
static SUPPORTED_BAUDS : &[u32] = &[115200];

pub fn determine_configuration(
    allowed_configs: &handshake::device::AllowedConfigs,
    _args: &Args,
) -> Option<handshake::host::UseConfig> {
    let mut allowed_versions : Vec<u16> = allowed_configs.supported_versions().to_vec();
    let mut allowed_bauds : Vec<u32> = allowed_configs.supported_bauds().to_vec();
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
    v == 1
}

fn is_baud_supported(&bd: &u32) -> bool {
    bd == 115200
}

pub fn try_promote(args: &Args, tty: &mut TTYPort) -> Result<bool> {
    // no error, but handshake failed  (no response)
    let Some(_config) = try_handshake(args, tty) else {
        log::info!("[{}]: Handshake failed.", bin_name());
        return Ok(false)
    };

    // TODO
    Ok(false)
}

pub fn try_handshake(args: &Args, tty: &mut TTYPort) -> Option<handshake::host::UseConfig> {
    if let Err(e) = send_message(handshake::host::Probe, tty) {
        log::error!("[host]: Failed to send probe: {e}");
        return None
    }

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
        Ok(t) => t,
        Err(e) => {
            log::error!("[host]: Failed to deserialize message: {e}");
            return None
        }
    };
    if !rem.is_empty() {
        log::error!("[host]: Bytes remaining in buffer after deserializing AllowedConfigs");
        return None
    }
    let Some(config) = determine_configuration(&ac, args) else {
        log::error!("[host]: No device-compatible version/baud configuration found");
        return None
    };

    log::info!("[host]: Settled on version {} at {}baud", config.version, config.baud);

    if let Err(e) = send_message(config, tty) {
        log::error!("[host]: Failed to send UseConfig: {e}");
        return None
    }

    Some(config)
}

pub trait Encode : serde::Serialize {
    fn msg_type(&self) -> u32;
    fn encode(&self) -> Result<Vec<u8>> {
        let buf = vec![];
        let buf = postcard::to_extend(&self.msg_type(), buf)?;
        let buf = postcard::to_extend(self, buf)?;
        Ok(buf)
    }
}

impl Encode for handshake::host::Probe {
    fn msg_type(&self) -> u32 {
        HandshakeMessageType::Probe.to_u32()
    }
}

impl Encode for handshake::host::UseConfig {
    fn msg_type(&self) -> u32 {
        HandshakeMessageType::UseConfig.to_u32()
    }
}

pub fn send_message<T: Encode>(msg: T, tty: &mut TTYPort) -> Result<()> {
    send_bytes(&msg.encode()?, tty)
}

pub fn send_bytes(bytes: &[u8], tty: &mut TTYPort) -> Result<()> {
    let mut frame = vec![];

    frame.extend_from_slice(&protocol::PREAMBLE.to_le_bytes());

    let bytes_crc = crc32fast::hash(bytes);

    let cobs_frame = {
        let mut cobs_frame = vec![];

        let mut cobs_buffer = [0u8; 254];
        let mut cobs_encoder = BufferedEncoder::with_buffer(&mut cobs_buffer).unwrap();
        let mut packet = cobs_encoder.packet();
        for &b in bytes.iter().chain(bytes_crc.to_le_bytes().iter()) {
            match packet.add_byte(b) {
                EncodeState::Buf(buf) => {
                    cobs_frame.extend_from_slice(buf);
                }
                EncodeState::Pass => {}
            }
        }
        cobs_frame.extend_from_slice(packet.finish());
        cobs_frame.iter_mut().for_each(|b| *b = *b ^ 0x55);
        cobs_frame
    };

    // LEB128
    frame.extend_from_slice(postcard::to_stdvec(&(cobs_frame.len() as u32)).unwrap().as_slice());
    frame.extend_from_slice(&cobs_frame);

    tty.write_all(&frame)
        .map_err(|e| eyre::eyre!("failed to send: {e}"))
}

pub fn recv_bytes_blocking_timeout(tty: &mut TTYPort, timeout: Duration) -> Result<Option<Vec<u8>>> {
    {
        let start = Instant::now();
        let mut state = 0;
        loop {
            if start.elapsed() > timeout {
                return Ok(None)
            }
            let byte = match tty.read8() {
                Ok(b) => b,
                Err(e) if e.kind() == ErrorKind::TimedOut => {
                    return Ok(None)
                }
                Err(e) if e.kind() == ErrorKind::BrokenPipe => {
                    log::error!("[{}]: Device disconnected. Aborting.", bin_name());
                    exit(1);
                }
                e @ Err(_) => e?
            };
            state = match (state, byte) {
                (0, 0x55) => 1,
                (1, 0x55) => 2,
                (2, 0x55) => 3,
                (3, 0x5e) => break,
                (3, 0x55) => 3,
                _ => 0,
            };
        }
    }

    let len = {
        let mut len = 0u32;
        for byte_no in 0..4 {
            let x = tty.read8()
                .with_context(|| if byte_no == 0 {
                    "while waiting for FRAME.LEN"
                } else {
                    "while reading FRAME.LEN"
                })?;
            let x = x ^ 0x55;
            if x == 0x00 {
                eyre::bail!("expected LEB128 byte, got 00, len={len} byte_no={byte_no}");
            }
            len |= ((x & 0x7f) as u32) << (7 & (byte_no as u32));
            if x & 0x80 == 0 {
                break
            } else if byte_no == 3 {
                eyre::bail!("expected LEB128-encoded 28-bit unsigned integer; did not terminate when expected; got len={len} so far");
            }
        }
        len as usize
    };

    let mut cobs_frame = vec![0; len];
    tty.read_exact(cobs_frame.as_mut())
        .with_context(|| "while waiting for FRAME.COBS_FRAME")?;
    // undo XOR55
    cobs_frame.iter_mut().for_each(|b| *b = *b ^ 0x55);
    // check CRC
    let message_crc = {
        let crc_bytes = &cobs_frame[(len - 4)..];
        u32::from_le_bytes([
            crc_bytes[0],
            crc_bytes[1],
            crc_bytes[2],
            crc_bytes[3]])
    };
    let calc_crc = crc32fast::hash(&cobs_frame[..(len-4)]);

    if message_crc == calc_crc {
        Ok(Some(cobs_frame[..(len-4)].to_vec()))
    } else {
        eyre::bail!("CRC mismatch: message had checksum {message_crc}, host computed {calc_crc}");
    }
}