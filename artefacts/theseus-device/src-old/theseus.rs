use bcm2835_lpa::{SYSTMR, UART1};
use crate::{boot_umsg, data_synchronization_barrier, uart1};
use crate::fmt::UartWrite;
use core::fmt::Write;
use thiserror::Error;
use theseus_common::cobs::{EncodeState, FeedState, LineDecoder};
use theseus_common::theseus::v1::{DEVICE_PROTOCOL_RESET_TIMEOUT, MESSAGE_PRECURSOR, MessageContent, RECEIVE_TIMEOUT};
use crate::delay::STInstant;

pub mod v1;

#[derive(Debug, Copy, Clone)]
enum LocalVersion {
    V1,
}

pub(crate) fn perform_download(
    uw: &mut UartWrite,
    uart: &UART1,
    st: &SYSTMR
) {
    // boot_umsg!(uw, "CRC_TEST : {}", crc32fast::hash(&[1,2,3,4]));
    // let m = crc32fast::hash(&[1,2,3,4]);
    // if m == 3057449933 {
    //     boot_umsg!(uw, "CRC=3057449933");
    // } else if m == 2555773972 {
    //     boot_umsg!(uw, "CRC=2555773972");
    // } else {
    //     boot_umsg!(uw, "idk wtf crc is");
    // }
    let Some(v) = perform_download_select_version(uw, uart, st) else {
        return
    };
    match v {
        LocalVersion::V1 => {
            v1::perform_download(uw, uart, st);
        }
    }
    boot_umsg!(uw, "[theseus-device]: theseus::perform_download failed out, retrying");
}

fn perform_download_select_version(
    uw: &mut UartWrite,
    uart: &UART1,
    st: &SYSTMR
) -> Option<LocalVersion> {
    let began_waiting_for_spv = STInstant::now(st);
    // we just received MESSAGE_PRECURSOR, so this time we directly eat the incoming message
    let mut buf = [0; v1::CTL_BUF_SIZE];
    let mut mres = uart1_recv_cobs_packet(
        uw,
        uart,
        &mut buf,
    ).map_err(WaitPacketError::RecvError);
    loop {
        if began_waiting_for_spv.elapsed(st) >= DEVICE_PROTOCOL_RESET_TIMEOUT {
            boot_umsg!(uw, "[theseus-device]: did not receive SetProtocolVersion within {}us, resetting.",
                DEVICE_PROTOCOL_RESET_TIMEOUT.as_micros());
            return None
        }
        let msg : Option<MessageContent> = match mres {
            Ok(l) => {
                postcard::from_bytes(&buf[..l])
                    .inspect_err(|e| {
                        boot_umsg!(uw, "[theseus-device]: deserialization failed for {:#x?}: {e}", &buf[..l]);
                    })
                    .ok()
            }
            Err(WaitPacketError::Timeout)=> {
                boot_umsg!(uw, "[theseus-device]: failed to read packet: timeout ({}us)", RECEIVE_TIMEOUT.as_micros());
                None
            }
            Err(WaitPacketError::RecvError(e)) => {
                boot_umsg!(uw, "[theseus-device]: failed to read packet: {e}, first bytes {} {} {} {}",
                    buf[0], buf[1], buf[2], buf[3]
                );
                None
            }
        };
        match msg {
            Some(MessageContent::SetProtocolVersion { version }) => {
                return match version {
                    1 => Some(LocalVersion::V1),
                    _ => {
                        boot_umsg!(uw, "[theseus-device]: unrecognized protocol version: {version}. aborting.");
                        None
                    }
                }
            }
            Some(_) => {
                boot_umsg!(uw, "[theseus-device]: unexpected message: {msg:?}");
            }
            None => {}
        }
        mres = uart1_wait_for_theseus_packet(
            uw,
            uart,
            st,
            &mut buf,
        );
    }
}

#[derive(Debug, Copy, Clone, Error)]
enum WaitPacketError {
    #[error("hit RECEIVE_TIMEOUT while waiting for MESSAGE_PRECURSOR")]
    Timeout,
    #[error("error receiving packet: {0}")]
    RecvError(#[from] RecvPacketError),
}

pub fn uart1_wait_for_theseus_packet(
    uw: &mut UartWrite,
    uart: &UART1,
    st: &SYSTMR,
    buf: &mut [u8],
) -> Result<usize, WaitPacketError> {
    let start = STInstant::now(st);
    let mut state = 0;
    loop {
        if start.elapsed(st) > RECEIVE_TIMEOUT {
            return Err(WaitPacketError::Timeout)
        }
        let Some(byte) = uart1::uart1_read8_nb(uart) else { continue };
        state = match (state, byte) {
            (0, 0x55) => 1,
            (1, 0x77) => 2,
            (2, 0xaa) => 3,
            (3, 0xff) => { break },
            _ => 0,
        }
    }
    uart1_recv_cobs_packet(uw, uart, buf)
        .map_err(WaitPacketError::RecvError)
}

#[derive(Debug, Copy, Clone, Error)]
enum RecvPacketError {
    // Packet unfinished
    #[error("packet unfinished: no SENTINEL byte")]
    TimeoutUnfinished,
    // No packet received
    #[error("no packet received within ITER_COUNT iterations")]
    TimeoutNoPacket,
    // Packet overflowed buffer
    #[error("packet too large for buffer")]
    Overflow,
    // Packet too small to contain a CRC
    #[error("packet too small to contain CRC32")]
    TooSmall,
    // CRC mismatch
    #[error("CRC mismatch: expected {expected} got {received}")]
    Crc32 { expected: u32, received: u32 },
}

pub fn hexify(s: &[u8]) -> crate::fmt::TinyBuf<0x500> {
    use core::fmt::Write;
    let mut t = crate::fmt::TinyBuf::<0x500>::default();
    for &b in s.iter() {
        let _ = t.write_str(HEX_TABLE[b as usize]);
        let _ = t.write_str(" ");
    }
    t
}

static HEX_TABLE : [&'static str; 256] = [
    "00", "01", "02", "03", "04", "05", "06", "07", "08", "09", "0a", "0b", "0c", "0d", "0e", "0f",
    "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "1a", "1b", "1c", "1d", "1e", "1f",
    "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "2a", "2b", "2c", "2d", "2e", "2f",
    "30", "31", "32", "33", "34", "35", "36", "37", "38", "39", "3a", "3b", "3c", "3d", "3e", "3f",
    "40", "41", "42", "43", "44", "45", "46", "47", "48", "49", "4a", "4b", "4c", "4d", "4e", "4f",
    "50", "51", "52", "53", "54", "55", "56", "57", "58", "59", "5a", "5b", "5c", "5d", "5e", "5f",
    "60", "61", "62", "63", "64", "65", "66", "67", "68", "69", "6a", "6b", "6c", "6d", "6e", "6f",
    "70", "71", "72", "73", "74", "75", "76", "77", "78", "79", "7a", "7b", "7c", "7d", "7e", "7f",
    "80", "81", "82", "83", "84", "85", "86", "87", "88", "89", "8a", "8b", "8c", "8d", "8e", "8f",
    "90", "91", "92", "93", "94", "95", "96", "97", "98", "99", "9a", "9b", "9c", "9d", "9e", "9f",
    "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9", "aa", "ab", "ac", "ad", "ae", "af",
    "b0", "b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8", "b9", "ba", "bb", "bc", "bd", "be", "bf",
    "c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "c9", "ca", "cb", "cc", "cd", "ce", "cf",
    "d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8", "d9", "da", "db", "dc", "dd", "de", "df",
    "e0", "e1", "e2", "e3", "e4", "e5", "e6", "e7", "e8", "e9", "ea", "eb", "ec", "ed", "ee", "ef",
    "f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "fa", "fb", "fc", "fd", "fe", "ff",
];

/// Note: does NOT process MESSAGE_PRECURSOR
/// Will abort if gap between symbols is >10us
pub fn uart1_recv_cobs_packet(
    _uw: &mut UartWrite,
    uart: &UART1,
    buf: &mut [u8],
) -> Result<usize, RecvPacketError> {
    // for higher throughput, we don't directly use the uart1::uart1_* functions.
    data_synchronization_barrier();

    // 250MHz
    // ~1 op/cycle, so at ~5 cycles, that's around 5cy/iter
    // 1 cy is 4ns, so 20ns/iter
    // so 500 iters is 10us
    // PROBLEM: not sure how what the latency on MMIO registers is

    const ITER_COUNT : usize = 500000;
    let mut cursor = 0;
    let mut line_decoder = LineDecoder::new();

    loop {
        let mut cycles = ITER_COUNT;
        while cycles > 0 {
            // probably something like
            // .L1:
            //  subs C, C, #1
            //  beq .timeout
            //  ldr A, [B, ?]
            //  tst A, #(1 << ?)
            //  beq .L1
            // .L1exit:
            //  ...
            // .timeout:
            //  ...
            if uart.stat().read().data_ready().bit_is_set() {
                break
            }
            cycles -= 1;
        }
        if cycles == 0 {
            data_synchronization_barrier();
            return if cursor > 0 {
                Err(RecvPacketError::TimeoutUnfinished)
            } else {
                Err(RecvPacketError::TimeoutNoPacket)
            }
        }
        let b = uart.io().read().data().bits();
        match line_decoder.feed(b) {
            FeedState::PacketFinished => {
                break
            }
            FeedState::Byte(b) => {
                if cursor == buf.len() {
                    data_synchronization_barrier();
                    return Err(RecvPacketError::Overflow)
                }
                buf[cursor] = b;
                cursor += 1;
            }
            FeedState::Pass => {
                continue
            }
        }
    }

    data_synchronization_barrier();

    if cursor < 4 {
        return Err(RecvPacketError::TooSmall)
    }
    let crc_slice = &buf[(cursor-4)..cursor];
    let crc_array = [crc_slice[0], crc_slice[1], crc_slice[2], crc_slice[3]];
    let message_crc32 = u32::from_le_bytes(crc_array);
    let content_slice = &buf[..(cursor-4)];
    let content_crc32 = crc32fast::hash(content_slice);

    if content_crc32 != message_crc32 {
        return Err(RecvPacketError::Crc32 {
            expected: message_crc32,
            received: content_crc32,
        })
    }

    Ok(cursor-4)
}

pub fn uart1_send_theseus_packet(
    uart: &UART1,
    data: &[u8],
) {
    let crc32 = crc32fast::hash(&data[..]);
    // let mut crc = crc32fast::Hasher::new();
    // crc.update(&data[..]);
    // let crc32: [u8; 4] = crc.finalize().to_le_bytes();

    let mut enc = theseus_common::cobs::BufferedEncoder::new();
    let mut p = enc.packet();

    uart1::uart1_write32(uart, MESSAGE_PRECURSOR);
    for &byte in data[..].iter().chain(crc32.to_le_bytes().iter()) {
        match p.add_byte(byte) {
            EncodeState::Buf(buf) => {
                uart1::uart1_write_bytes(uart, buf);
            }
            EncodeState::Pass => {}
        }
    }
    uart1::uart1_write_bytes(uart, p.finish());
}

