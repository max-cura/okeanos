use crate::tty::Tty;
use crate::Args;
use eyre::{bail, ensure, eyre, Context, Result};
use musli::mode::Binary;
use musli::Encode;
use okboot_common::device::AllowedVersions;
use okboot_common::frame::{
    BufferedEncoder, EncodeState, FrameEncoder, FrameError, FrameLayer, FrameOutput,
};
use okboot_common::host::UseVersion;
use okboot_common::{EncodeMessageType, MessageType, COBS_XOR};
use std::fmt::Debug;
use std::io::{ErrorKind, Read, Write};
use std::process::exit;
use std::time::{Duration, Instant};
use tracing::instrument;

const TTY_TIMEOUT: Duration = Duration::from_millis(100);
const PROMOTION_TRIES: usize = 5;

pub fn upload(args: Args) -> Result<()> {
    let mut tty = Tty::new(&args.device, okboot_common::INITIAL_BAUD_RATE)?;
    tty.set_timeout(TTY_TIMEOUT)?;

    // Two things we do here:
    //  first, we try to upgrade out of the LEGACY protocol, trying N number of times
    //  second, we try to upload using the legacy protocol

    enum Mode {
        Legacy,
        Okdude(UseVersion),
    }
    let mut mode = Mode::Legacy;
    for attempt in 1..=PROMOTION_TRIES {
        if let Some(version) = try_promotion_handshake(&args, &mut tty) {
            mode = Mode::Okdude(version);
            break;
        } else {
            tracing::warn!("failed attempt {attempt}/{PROMOTION_TRIES} to promote protocol");
        }
    }
    match mode {
        Mode::Legacy => {
            tracing::warn!("attempting upload using SU-BOOT protocol");
        }
        Mode::Okdude(version) => {
            tracing::debug!("using okdude protocol version {:08x}", version.version);
        }
    }

    Ok(())
}

fn try_promotion_handshake(args: &Args, tty: &mut Tty) -> Option<UseVersion> {
    const PROMOTION_RECV_TIMEOUT: Duration = Duration::from_millis(100);
    if let Err(e) = send(&okboot_common::host::Probe {}, tty) {
        tracing::error!("[host]: failed to send Probe: {e}");
        return None;
    }

    let msg = match recv_with_print_string(tty, PROMOTION_RECV_TIMEOUT) {
        Ok(Some(m)) => m,
        Ok(None) => {
            tracing::debug!("[host]: received no AllowedVersions within timeout.");
            return None;
        }
        Err(e) => {
            tracing::error!("[host]: failed to receive message: {e}");
            return None;
        }
    };
    if msg.0 != MessageType::AllowedVersions {
        tracing::error!(
            "[host]: received message type {:?} in response to Probe",
            msg.0
        );
        return None;
    }
    let allowed_versions: AllowedVersions = match musli::wire::decode(&*msg.1) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("[host]: failed to deserialize AllowedVersions message payload: {e}");
            return None;
        }
    };
    let allowed_versions: Vec<u32> = allowed_versions.iter().collect();
    static SUPPORTED_VERSIONS: &[u32] = &[0x0000_0002];
    let choice = allowed_versions
        .iter()
        .rev()
        .find(|x| SUPPORTED_VERSIONS.contains(*x))
        .copied();
    let Some(version) = choice else {
        tracing::error!("[host]: allowed versions: {allowed_versions:?}, supported versions: {SUPPORTED_VERSIONS:?}");
        return None;
    };

    let config = UseVersion { version };
    if let Err(e) = send(&config, tty) {
        tracing::error!("[host]: failed to send UseVersion: {e}");
        return None;
    }
    Some(config)
}

/// Special cased blocking recv with timeout that handles `PRINT_STRING`s.
#[instrument(skip(tty))]
fn recv_with_print_string(
    tty: &mut Tty,
    timeout: Duration,
) -> Result<Option<(MessageType, Vec<u8>)>> {
    // can't use the normal decoder because we might get `PRINT_STRING`s
    let start = Instant::now();
    let mut state = 0;
    loop {
        if start.elapsed() > timeout {
            tracing::trace!("receive timeout, canceling recv");
            return Ok(None);
        }
        let byte = match tty.read8() {
            Ok(b) => b,
            Err(e) if e.kind() == ErrorKind::TimedOut => {
                tracing::trace!("read timeout");
                continue;
            }
            Err(e) if e.kind() == ErrorKind::BrokenPipe => {
                tracing::trace!("device disconnected. aborting.");
                exit(1)
            }
            e @ Err(_) => e?,
        };
        state = match (state, byte) {
            (0, 0x55) => 1,
            (1, 0x55) => 2,
            (2, 0x55) => 3,
            (3, 0x53) => break,
            (3, 0x55) => 3,
            (0, 0xee) => 5,
            (5, 0xee) => 6,
            (6, 0xee) => 6,
            (6, 0xdd) => 7,
            (7, 0xdd) => {
                let len = tty.read32_le().unwrap_or(0);
                if len > 0 {
                    let mut v = vec![0; len as usize];
                    let _ = tty
                        .read_exact(&mut v[..])
                        .inspect_err(|e| tracing::error!("failed to read in PRINT_STRING: {e}"));
                    tracing::info!("< {}", String::from_utf8_lossy(&v));
                }
                0
            }
            _ => 0,
        };
    }

    let mut frame_decoder = FrameLayer::new(COBS_XOR);
    frame_decoder.skip_preamble();
    // restart timeout from end of preamble
    let start = Instant::now();
    let mut header = None;
    let mut payload = vec![];
    loop {
        if start.elapsed() > timeout {
            bail!("receive timeout (2), canceling recv in preamble");
        }
        let byte = match tty.read8() {
            Ok(b) => b,
            Err(e) if e.kind() == ErrorKind::TimedOut => {
                tracing::trace!("read timeout");
                continue;
            }
            Err(e) if e.kind() == ErrorKind::BrokenPipe => {
                tracing::trace!("device disconnected. aborting.");
                exit(1)
            }
            e @ Err(_) => e?,
        };
        let out = match frame_decoder.poll(byte) {
            Ok(s) => s,
            Err(e) => {
                return Err(e).wrap_err("failed to receive frame");
            }
        };
        match out {
            FrameOutput::Skip => {}
            FrameOutput::Header(hdr) => {
                if let Some(orig) = header.replace(hdr) {
                    bail!("received second header: {hdr:?}, original was {orig:?}");
                }
            }
            FrameOutput::Payload(x) => {
                let header = header.ok_or_else(|| eyre!("frame finished with no header"))?;
                payload.push(x);
                if header.payload_len < payload.len() {
                    bail!("frame length mismatch: declared payload length {} bytes, just received byte #{}", header.payload_len, payload.len());
                }
            }
            FrameOutput::Finished => {
                let header = header.ok_or_else(|| eyre!("frame finished with no header"))?;
                if header.payload_len != payload.len() {
                    bail!("frame length mismatch: declared payload length {} bytes, received {} bytes", header.payload_len, payload.len());
                }
                return Ok(Some((header.message_type, payload)));
            }
        }
    }
}

fn send<M: EncodeMessageType + musli::Encode<Binary> + Debug>(
    message: &M,
    tty: &mut Tty,
) -> Result<()> {
    // serialize the message
    let mut serialized_message = Vec::new();
    musli::wire::encode(&mut serialized_message, message)
        .wrap_err(eyre!("failed to encode message <{message:?}>"))?;
    // derive message type and payload length
    let message_type = <M as EncodeMessageType>::TYPE;
    let payload_len = serialized_message.len();
    // build frame that we're COBS-encoding
    let mut message_bytes = vec![];
    message_bytes.extend_from_slice(&u32::from(message_type).to_le_bytes());
    message_bytes.extend_from_slice(
        &u32::try_from(payload_len)
            .expect("message payload is >=4GiB")
            .to_le_bytes(),
    );
    message_bytes.extend_from_slice(&serialized_message);

    // build the final message we're sending over the wire
    let mut wire_bytes = vec![];
    // first, the preamble
    wire_bytes.extend_from_slice(&okboot_common::PREAMBLE_BYTES);
    // COBS-encode the `message_bytes`
    let mut buf = [0; 255];
    let mut buf_enc = BufferedEncoder::with_buffer_xor(&mut buf[..], okboot_common::COBS_XOR);
    let mut frame_encoder = buf_enc.frame().expect("failed to build frame encoder");
    for byte in message_bytes {
        match frame_encoder.write_u8(byte) {
            EncodeState::Buf(b) => {
                wire_bytes.extend_from_slice(b);
            }
            EncodeState::Pass => continue,
        }
    }
    wire_bytes.extend_from_slice(frame_encoder.finish());

    // write to the TTY
    tty.write_all(&wire_bytes)
        .wrap_err(eyre!("failed to write message <{message:?}>"))?;
    // make sure to flush
    tty.flush()
        .wrap_err(eyre!("failed to flush message <{message:?}>"))?;

    // finished
    Ok(())
}
