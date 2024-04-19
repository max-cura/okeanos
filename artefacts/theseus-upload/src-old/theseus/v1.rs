use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::process::exit;
use std::time::Duration;
use color_eyre::eyre;
use serialport::{SerialPort, TTYPort};
use theseus_common::cobs::{BufferedEncoder, EncodeState, FeedState, LineDecoder};
use theseus_common::theseus::{ProgramCRC32, TheseusVersion};
use theseus_common::theseus::v1::{SET_BAUD_RATE_TIMEOUT, MESSAGE_PRECURSOR, MessageContent, RETRY_ATTEMPTS_CAP, RECEIVE_TIMEOUT};
use crate::args::Args;
use crate::bin_name;
use crate::io::RW32;

#[derive(Debug, Copy, Clone)]
enum State {
    // sending SetProtocolVersion, waiting for RequestProgramInfoRPC
    // transitions to SettingBaudRate
    SettingProtocolVersion,
    // sending SetBaudRateRPC, waiting for BaudRateAck
    // transitions to TransitioningBaudRate
    SettingBaudRate,

    // // sent BaudRateReady, switched rate, waiting for RequestProgramInfoRPC in new baud rate
    // // transitions to SendingProgramInfo or SettingBaudRate
    // TransitioningBaudRate,

    // sending ProgramInfo, waiting for RequestProgramRPC
    // transitions to SendingProgramReady
    SendingProgramInfo,
    // sending ProgramReady, waiting for ReadyForChunk
    // transitions to SendingProgramChunk
    SendingProgramReady,
    // sending ProgramChunk, waiting for ReadyForChunk or ProgramReceived
    // transitions to SendingProgramChunk or EXITS
    SendingProgramChunk,
}
impl State {
    pub fn waiting_for(self) -> &'static str {
        match self {
            State::SettingProtocolVersion => {"RequestProgramInfoRPC"}
            State::SettingBaudRate => {"BaudRateAck"}
            // State::TransitioningBaudRate => {"RequestProgramInfoRPC (new rate)"}
            State::SendingProgramInfo => {"RequestProgramRPC"}
            State::SendingProgramReady => {"ReadyForChunk"}
            State::SendingProgramChunk => {"ReadyForChunk or ProgramReceived"}
        }
    }
}

pub(crate) fn dispatch(args: &Args, tty: &mut TTYPort) -> eyre::Result<()> {
    tty.set_timeout(RECEIVE_TIMEOUT)?;

    let raw_program_data = std::fs::read(args.bin_file.as_path())?;
    let program_data = {
        raw_program_data.clone()
        // miniz_oxide::deflate::compress_to_vec(&raw_program_data, miniz_oxide::deflate::CompressionLevel::BestSpeed as u8)
    };
    let program_crc32 = {
        let mut crc = ProgramCRC32::new();
        crc.add_data(&raw_program_data);
        crc.finalize()
    };

    let mut incoming_msgs_buf : VecDeque<MessageRaw> = VecDeque::new();
    let mut outgoing_msg : MessageContent;
    let mut state : State;

    let mut chunk_size = 0;

    log::info!("[{}]: Setting device to THESEUSv1 protocol", bin_name());
    state = State::SettingProtocolVersion;
    outgoing_msg = MessageContent::SetProtocolVersion {
        version: 1,
    };

    let mut retries_since_last_received = 0;
    // we don't track retries during program transmission, since we don't know how long CRC or
    // decompression will take.
    let mut track_retries = true;

    'outer: loop {
        send_message(tty, &outgoing_msg).inspect_err(|e| {
            log::error!("[{}]: Failed to send message: {}", bin_name(), e);
        })?;

        if !try_recv_messages(tty, &mut incoming_msgs_buf)? {
            if track_retries {
                if retries_since_last_received == RETRY_ATTEMPTS_CAP {
                    log::error!("[{}]: Failed to read after {} retries. Aborting.", bin_name(), RETRY_ATTEMPTS_CAP);
                    exit(1);
                }
                log::trace!("[{}]: Hit read timeout, retrying ({retries_since_last_received}/{})", bin_name(), RETRY_ATTEMPTS_CAP);
                retries_since_last_received += 1;
            } else {
                // log::trace!("[{}]: Hit read timeout, retrying (unlimited).", bin_name())
            }
            continue
        } else {
            retries_since_last_received = 0;
        }
        while let Some(incoming_msg_buf) = incoming_msgs_buf.pop_front() {
            let msg = postcard::from_bytes(&incoming_msg_buf.content)
                .map_err(|e| eyre::eyre!("try_recv_message: Deserialization failed: {e}"))?;

            match msg {
                // Device->Host messages
                MessageContent::PrintMessageRPC { message } => {
                    // special message that can come in at any time
                    log::info!("[{}]: < {}", bin_name(), message);
                    // .map(ToString::to_string)
                    // .unwrap_or_else(|e| format!("<invalid UTF-8: {e}>")));
                }
                MessageContent::RequestProgramInfoRPC => {
                    // expected in TransitioningBaudRate and SettingProtocolVersion
                    match state {
                        // State::TransitioningBaudRate => {
                        //     // todo!()
                        // }
                        State::SettingProtocolVersion => {
                            if args.baud == tty.baud_rate().unwrap() {
                                // go straight to SendingProgramInfo
                                log::debug!("transition: SettingProtocolVersion -> SendingProgramInfo");
                                state = State::SendingProgramInfo;
                                outgoing_msg = MessageContent::ProgramInfo {
                                    load_at_address: args.address,
                                    // THIS IS CORRECT: ENCODED LENGTH
                                    program_size: program_data.len() as u32,
                                    program_crc32,
                                };
                            } else {
                                // go to SetBaudRateRPC
                                log::debug!("transition: SettingProtocolVersion -> SettingBaudRate");
                                state = State::SettingBaudRate;
                                outgoing_msg = MessageContent::SetBaudRateRPC {
                                    baud_rate: args.baud,
                                };
                            }
                        }
                        _ => { log::error!("[{}]: Host received unexpected {msg:?} while waiting for {}", bin_name(), state.waiting_for()); }
                    }
                }
                MessageContent::BaudRateAck { possible } => {
                    // expected in SettingBaudRate
                    match state {
                        State::SettingBaudRate => {
                            // acknowledge we saw BaudRateAck
                            send_message(tty, &MessageContent::BaudRateReady).inspect_err(|e| {
                                log::error!("[{}]: Failed to send message: {}", bin_name(), e);
                            })?;

                            if possible {
                                // received ack
                                log::debug!("[{}]: Attempting to set baud rate to {}", bin_name(), args.baud);
                                tty.set_baud_rate(args.baud)?;

                                let brr_send_time = std::time::Instant::now();

                                let mut succeeded = false;

                                'brr: while brr_send_time.elapsed() < SET_BAUD_RATE_TIMEOUT {
                                    let mut v = VecDeque::new();
                                    if !try_recv_messages(tty, &mut v)? { continue 'brr }
                                    while let Some(msg_buf) = v.pop_front() {
                                        if let Ok(msg) = postcard::from_bytes::<MessageContent>(&msg_buf.content)
                                            .inspect_err(|e| log::error!("try_recv_message: Deserialization failed: {e} (continuing : corruption possible due to baud rate negotiation)"))
                                        {
                                            if let MessageContent::RequestProgramInfoRPC { .. } = msg {
                                                succeeded = true;
                                                break 'brr
                                            } else {
                                                continue
                                            }
                                        }
                                    }
                                }

                                if succeeded {
                                    log::info!("[{}]: Set baud rate to {}", bin_name(), args.baud);
                                    log::debug!("transition: SettingBaudRate -> SendingProgramInfo");
                                    state = State::SendingProgramInfo;
                                    outgoing_msg = MessageContent::ProgramInfo {
                                        load_at_address: args.address,
                                        // THIS IS CORRECT: ENCODED LENGTH
                                        program_size: program_data.len() as u32,
                                        program_crc32,
                                    };
                                } else {
                                    log::error!("[{}]: Encountered timeout waiting for baud rate change. Retrying.", bin_name());

                                    tty.set_baud_rate(115200)?;

                                    // no state change, no outgoing message change.
                                }
                            } else {
                                log::error!("[{}]: Failed to set baud rate: device error. Aborting.", bin_name());
                                exit(1);
                            }
                        }
                        _ => { log::error!("[{}]: Host received unexpected {msg:?} while waiting for {}", bin_name(), state.waiting_for()); }
                    }
                }
                MessageContent::RequestProgramRPC { crc_retransmission, chunk_size: chsz } => {
                    // expected in SendingProgramInfo
                    match state {
                        State::SendingProgramInfo => {
                            if crc_retransmission != program_crc32 {
                                log::error!("[{}]: Received corrupted retransmitted CRC in RequestProgramRPC: expected {program_crc32} received {crc_retransmission}", bin_name());
                            } else {
                                log::debug!("transition: SendingProgramInfo -> SendingProgramReady");
                                log::info!("[{}]: Setting chunk size to {chsz}", bin_name());
                                state = State::SendingProgramReady;
                                outgoing_msg = MessageContent::ProgramReady;
                                chunk_size = chsz as usize;
                            }
                        }
                        _ => { log::error!("[{}]: Host received unexpected {msg:?} while waiting for {}", bin_name(), state.waiting_for()); }
                    }
                }
                MessageContent::ReadyForChunk { chunk_no } => {
                    track_retries = false;
                    // expected in SendingProgramReady and SendingProgramChunk
                    match state {
                        State::SendingProgramReady | State::SendingProgramChunk => {
                            let offset = (chunk_no as usize) * chunk_size;
                            if offset >= program_data.len() {
                                log::error!("[{}]: Invalid chunk_no: {chunk_no} when program only has {} chunks", bin_name(), (program_data.len() + chunk_size - 1) / chunk_size);
                            } else {
                                let end = (offset + (chunk_size)).min(program_data.len());
                                let chunk_data = &program_data[offset..end];
                                if match outgoing_msg {
                                    MessageContent::ProgramChunk { chunk_no: pc_chunkno, .. } => chunk_no > pc_chunkno,
                                    _ => true
                                } {
                                    log::info!("[{}]: Sending chunk #{chunk_no}: {}", bin_name(), hexify(chunk_data));
                                }
                                if matches!(state, State::SendingProgramReady) {
                                    log::debug!("transition: SendingProgramReady -> SendingProgramChunk");
                                    state = State::SendingProgramChunk;
                                }
                                outgoing_msg = MessageContent::ProgramChunk {
                                    chunk_no,
                                    data: chunk_data,
                                };
                            }
                        }
                        _ => { log::error!("[{}]: Host received unexpected {msg:?} while waiting for {}", bin_name(), state.waiting_for()); }
                    }
                }
                MessageContent::ProgramReceived => {
                    // expected in SendingProgramChunk
                    match state {
                        State::SendingProgramChunk => {
                            log::debug!("transition: SendingProgramChunk -> DONE");
                            log::info!("[{}]: Finished uploading.", bin_name());
                            break 'outer
                        }
                        _ => { log::error!("[{}]: Host received unexpected {msg:?} while waiting for {}", bin_name(), state.waiting_for()); }
                    }
                }

                // Host->Device only messages
                MessageContent::SetProtocolVersion { .. }
                | MessageContent::SetBaudRateRPC { .. }
                | MessageContent::ProgramInfo { .. }
                | MessageContent::ProgramReady { .. }
                | MessageContent::ProgramChunk { .. }
                | MessageContent::BaudRateReady
                => {
                    log::error!("[{}]: Host received unexpected {msg:?}: message type is Host->Device only", bin_name())
                }
            }
        }
    }

    Ok(())
}

fn try_recv_messages<'a>(
    tty: &'_ mut TTYPort, buf: &'a mut VecDeque<MessageRaw>
) -> eyre::Result<bool> {
    let raw = match recv_message_raw(tty) {
        Ok(v) => if v.is_empty() {
            log::trace!("failed to read from tty: ignoring.");
            return Ok(false)
        } else {
            v
        }
        Err(e) => {
            log::error!("[{}]: failed to read from tty: {e}", bin_name());
            Err(e)?
        }
    };
    buf.extend(raw);
    Ok(!buf.is_empty())
}


pub fn send_message(tty: &mut TTYPort, msg: &MessageContent) -> eyre::Result<()> {
    // PRECURSOR [MESSAGE]
    tty.write32_le(MESSAGE_PRECURSOR)?;
    let mut v = postcard::to_stdvec(msg)?;
    let crc = crc32fast::hash(&v);
    v.extend(crc.to_le_bytes());
    // log::trace!("--) {}", hexify(&v));
    let mut v_cobs = Vec::new();
    let mut encode_buf = [0u8 ; 254];
    {
        let mut be = BufferedEncoder::with_buffer(&mut encode_buf).unwrap();
        let mut p = be.packet();
        for b in v {
            match p.add_byte(b) {
                EncodeState::Buf(s) => {
                    v_cobs.extend_from_slice(s);
                }
                EncodeState::Pass => {}
            }
        }
        v_cobs.extend_from_slice(p.finish());
    }

    log::trace!("> {}", hexify(&v_cobs));
    tty.write_all(&v_cobs)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct MessageRaw {
    pub content: Vec<u8>,
    // pub message_crc32: u32,
    // pub content_crc32: u32,
}

// Ok(vec.is_empty) means timeout
pub fn recv_message_raw(tty: &mut TTYPort) -> eyre::Result<Vec<MessageRaw>> {
    // wait on MESSAGE_PRECURSOR
    let mut mp_state = 0;
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > RECEIVE_TIMEOUT {
            return Ok(vec![])
        }
        let byte = match tty.read8() {
            Ok(b) => b,
            Err(e) if e.kind() == ErrorKind::TimedOut => {
                return Ok(vec![])
            }
            Err(e) if e.kind() == ErrorKind::BrokenPipe => {
                log::error!("[{}]: Device disconnected. Aborting.", bin_name());
                exit(1);
            }
            e @ Err(_) => e?
        };
        mp_state = match (mp_state, byte) {
            (0, 0x55) => 1,
            (1, 0x77) => 2,
            (2, 0xaa) => 3,
            (3, 0xff) => break,
            _ => 0,
        };
    }

    // okay, so we caught a message
    // now, we need to decode
    let mut raw_buf = [0u8; 32];
    let mut buf = Vec::new();

    // we can assume that there'll be at least ~10ms delay before retransmit, so just pull the whole
    // thing in at once
    while let Ok(ct) = tty.read(&mut raw_buf) {
        if ct == 0 {
            break
        }
        buf.extend_from_slice(&raw_buf[..ct])
    }

    // log::trace!("--[ {} ]", hexify(&buf));
    let mut line_decoder = LineDecoder::new();

    let mut i = 0;

    let mut packets = Vec::new();

    loop {
        if i == buf.len() {
            break
        }
        if (buf.len() - i) < 4 {
            log::error!("[{}]: remaining buffer is not a packet (too short): {}.", bin_name(), hexify(&buf[i..]));
            break
        }
        if i != 0 {
            if &buf[i..i + 4] == &[0x55, 0x77, 0xaa, 0xff] {
                i += 4;
            } else {
                log::error!("[{}]: packet does not begin with MESSAGE_PRECURSOR: remaining buffer {}.", bin_name(), hexify(&buf[i..]));
                log::error!("[{}]: decoder in disorder, dumping remaining buffer.", bin_name());
                break
            }
        }
        let mut buf_unstuffed = Vec::new();
        while i < buf.len() {
            let byte = buf[i];
            i += 1;
            match line_decoder.feed(byte) {
                FeedState::PacketFinished => { break }
                FeedState::Byte(b) => { buf_unstuffed.push(b) }
                FeedState::Pass => { continue }
            }
        }
        if buf[i - 1] != 0 {
            log::error!("[{}]: COBS packet empty or incomplete: {}", bin_name(), hexify(&buf_unstuffed));
            continue
        }

        log::trace!("--< COBS_DECODED {}", hexify(&buf_unstuffed));
        log::trace!("--( COBS_DECODED_ASCII {}",
            buf_unstuffed
                .iter()
                .map(|&b| if b.is_ascii_graphic() { b.as_ascii().unwrap().to_char() } else { '.' })
                .collect::<String>()
        );

        if buf_unstuffed.len() < 4 {
            eyre::bail!("Message too small: {}", hexify(&buf_unstuffed));
        }

        // last 4 bytes are CRC32
        let crc_begin = buf_unstuffed.len() - 4;
        let crc_slice = &buf_unstuffed[crc_begin..];
        let crc_array = [crc_slice[0], crc_slice[1], crc_slice[2], crc_slice[3]];
        let message_crc32 = u32::from_le_bytes(crc_array);

        let content_slice = &buf_unstuffed[..crc_begin];
        let content_crc32 = crc32fast::hash(content_slice);

        if content_crc32 != message_crc32 {
            log::error!("[{}]: CRC mismatch: expected {} got {}", bin_name(), message_crc32, content_crc32);
            continue
        }

        packets.push(MessageRaw { content: content_slice.to_vec() });
    }

    Ok(packets)
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
pub fn hexify(data: &[u8]) -> String {
    data.iter().map(|b| HEX_TABLE[*b as usize]).intersperse(" ").collect::<String>()
}