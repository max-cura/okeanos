use std::collections::VecDeque;
use std::io::{Read, Write};
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError, TryLockResult};
use serialport::{SerialPort, TTYPort};
use crate::args::Args;

use color_eyre::Result;
use crossbeam_channel::{RecvError, TryRecvError};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use theseus_common::cobs::{FeedState, LineDecoder};
use theseus_common::INITIAL_BAUD_RATE;
use theseus_common::theseus::{MessageClass, MessageTypeType, MSG_PRINT_STRING, v1};
use theseus_common::theseus::v1::{device, host};
use crate::hexify::hexify;
use crate::io::RW32;
use crate::theseus::encode::HostEncode;
use crate::tty::TTY;

pub struct TTYDriver<'a> {
    tty: &'a mut TTY,

    in_queue: crossbeam_channel::Sender<(MessageTypeType, Vec<u8>)>,
    out_queue: crossbeam_channel::Receiver<Vec<u8>>,

    close: Arc<AtomicBool>,
    state: S,
    decoder: LineDecoder,
    partial_frame: Vec<u8>,
}

#[derive(Debug, Copy, Clone)]
enum S {
    Waiting,
    Preamble1,
    Preamble2,
    Preamble3,
    Len0,
    Len1(u8),
    Len2(u8, u8),
    Len3(u8, u8, u8),
    CobsFrame { total_enc: usize, received: usize },

    // legacy
    PS1,
    PS2,
    PS3,
    PSLen0,
    PSLen1(u8),
    PSLen2(u8, u8),
    PSLen3(u8, u8, u8),
    PSFrame { total_len: usize, received: usize },
}

impl<'a> TTYDriver<'a> {
    pub fn receive_bytes(&mut self, buf: &[u8]) {
        let mut i = 0;
        while i < buf.len() {
            let byte = buf[i];
            i += 1;
            self.state = match (byte, self.state) {
                (0xee, S::Waiting) => S::PS1,
                (0xee, S::PS1) => S::PS2,
                (0xdd, S::PS2) => S::PS3,
                (0xdd, S::PS3) => S::PSLen0,

                (b0, S::PSLen0) => S::PSLen1(b0),
                (b1, S::PSLen1(b0)) => S::PSLen2(b0, b1),
                (b2, S::PSLen2(b0, b1)) => S::PSLen3(b0, b1, b2),
                (b3, S::PSLen3(b0, b1, b2)) => S::PSFrame { total_len: u32::from_le_bytes([b0, b1, b2, b3]) as usize, received: 0 },
                (b, S::PSFrame { total_len, received }) => {
                    self.partial_frame.push(b);
                    let received = received + 1;
                    if received == total_len {
                        log::info!("< {}", String::from_utf8_lossy(&self.partial_frame));
                        self.partial_frame.clear();
                        S::Waiting
                    } else {
                        S::PSFrame { total_len, received }
                    }

                    //let len = self.tty.read32_le().unwrap_or(0);
                    // if len > 0 {
                    //     let mut v = vec![0; len as usize];
                    //     let _ = self.tty.read_exact(&mut v)
                    //         .inspect_err(|e| log::error!("(host:v1) PRINT_STRING read_exact of {len} failed: {e}"));
                    //     // log::trace!("< {}", hexify(&v));
                    //     log::info!("< {}", String::from_utf8_lossy(&v));
                    // }
                    // S::Waiting
                }

                (0x55, S::Waiting) => S::Preamble1,
                (0x55, S::Preamble1) => S::Preamble2,
                (0x55, S::Preamble2) => S::Preamble3,
                (0x55, S::Preamble3) => S::Preamble3,
                (0x5e, S::Preamble3) => S::Len0,

                (b0, S::Len0) => S::Len1(b0),
                (b1, S::Len1(b0)) => S::Len2(b0, b1),
                (b2, S::Len2(b0, b1)) => S::Len3(b0, b1, b2),
                (b3, S::Len3(b0, b1, b2)) => {
                    let len = theseus_common::theseus::len::decode_len(&[b0, b1, b2, b3]);
                    if len < 4 {
                        log::error!("[host:v1]: frame length is less than 4");
                        S::Waiting
                    } else {
                        self.decoder.reset();
                        self.partial_frame.clear();
                        S::CobsFrame { total_enc: len as usize, received: 0 }
                    }
                }

                (b, S::CobsFrame { total_enc, received }) => {
                    if received >= total_enc {
                        log::error!("[host:v1]: COBS frame longer than expected: expected {total_enc} bytes");
                        S::Waiting
                    } else {
                        let byte = b ^ 0x55;
                        match self.decoder.feed(byte) {
                            FeedState::PacketFinished => 'packet: loop {
                                if total_enc != (received + 1) {
                                    log::error!("[host:v1]: COBS frame shorter than expected: expected {total_enc} bytes got {}", received+1);
                                    break 'packet S::Waiting;
                                }
                                let crc_bytes: [u8; 4] = self.partial_frame[self.partial_frame.len() - 4..].try_into().unwrap();
                                let declared_crc = u32::from_le_bytes(crc_bytes);
                                let data_frame_bytes = &self.partial_frame[..self.partial_frame.len() - 4];
                                let computed_crc = crc32fast::hash(data_frame_bytes);
                                if declared_crc != computed_crc {
                                    log::error!("[host:v1]: CRC mismatch: expected {declared_crc} got {computed_crc}");
                                    break 'packet S::Waiting;
                                }
                                let (typ, rem) = match postcard::take_from_bytes::<MessageTypeType>(data_frame_bytes) {
                                    Ok(x) => x,
                                    Err(e) => {
                                        log::error!("[host:v1]: Failed deserialization of message type: {e}");
                                        break 'packet S::Waiting;
                                    }
                                };
                                {
                                    if typ == MSG_PRINT_STRING {
                                        log::info!("< {}", std::str::from_utf8(rem).unwrap_or("<invalid UTF-8>"));
                                    } else if let Err(e) = self.in_queue.send((typ, rem.to_vec())) {
                                        log::error!("[host:v1]: Failed to queue incoming message: {e}");
                                        break 'packet S::Waiting;
                                    }
                                    // let mut l = self.in_queue.lock().unwrap();
                                    // l.push((typ, rem.to_vec()));
                                    // drop(l);
                                }
                                self.partial_frame.clear();
                                break 'packet S::Waiting;
                            }
                            FeedState::Byte(b) => {
                                self.partial_frame.push(b);
                                S::CobsFrame { total_enc, received: received + 1 }
                            }
                            FeedState::Pass => S::CobsFrame { total_enc, received: received + 1 },
                        }
                    }
                }

                (x, state) => {
                    log::error!("[host:v1]: Unexpected byte in state {state:?}: {x}");
                    S::Waiting
                }
            }
        }
    }

    pub fn drive(&mut self) {
        'drive: loop {
            if self.close.load(Ordering::SeqCst) {
                break 'drive;
            }

            {
                // TODO: flushing?
                while !self.out_queue.is_empty() {
                    // log::trace!("[host:v1]: Beginning send at {:?}",
                    //         std::time::Instant::now());
                    match self.out_queue.recv() {
                        Ok(f) => {
                            if let Err(e) = self.tty.write_all(&f) {
                                log::error!("[host:v1]: Failed to write queued message: {e}");
                            }
                            if let Err(e) = self.tty.flush() {
                                log::error!("[host:v1]: Failed to flush message");
                            }
                        }
                        Err(e) => {
                            panic!("TTYStream::drive: channel receive error: {e}");
                        }
                    }
                    // log::trace!("[host:v1]: Finished sent at {:?}",
                    //     std::time::Instant::now());
                }
            }

            // so, self.tty.bytes_to_read() doesn't really seem to be working quite right...
            // instead, just try plain old read() and see what happens

            if let Ok(can_read_n) = self.tty.bytes_to_read() {
                if can_read_n > 0 {
                    let mut vdrive = vec![0; can_read_n as usize];
                    let buf = match self.tty.read(&mut vdrive) {
                        Ok(buf_len) => &vdrive[..buf_len],
                        Err(e) => {
                            log::error!("[host:v1]: Failed to read from TTY: {e}");
                            continue 'drive;
                        }
                    };

                    // log::trace!("[ BYTES: {}", hexify(buf));

                    self.receive_bytes(buf);
                }
            }
        }
    }
}

impl HostEncode for host::ProgramInfo {}

impl HostEncode for host::ProgramReady {}

impl<'a> HostEncode for host::Chunk<'a> {}

impl HostEncode for device::RequestProgramInfo {}

impl HostEncode for device::RequestProgram {}

impl HostEncode for device::RequestChunk {}

impl HostEncode for device::Booting {}

pub struct TTYStream {
    in_queue: crossbeam_channel::Receiver<(MessageTypeType, Vec<u8>)>,
    out_queue: crossbeam_channel::Sender<Vec<u8>>,

    close: Arc<AtomicBool>,
}

impl TTYStream {
    pub fn send<T: HostEncode>(&mut self, msg: &T) -> Result<()> {
        let frame = super::encode::frame_bytes(&msg.encode()?)?;
        self.out_queue.send(frame)?;
        Ok(())
    }
    pub fn try_recv(&mut self) -> Option<(MessageTypeType, Vec<u8>)> {
        self.in_queue.try_recv().ok()
    }
}

pub fn split(tty: &mut TTY) -> (TTYDriver, TTYStream) {
    let close = Arc::new(AtomicBool::new(false));
    let (iq_tx, iq_rx) = crossbeam_channel::unbounded();
    let (oq_tx, oq_rx) = crossbeam_channel::unbounded();

    (
        TTYDriver {
            tty,
            in_queue: iq_tx,
            out_queue: oq_rx,
            close: close.clone(),
            state: S::Waiting,
            decoder: LineDecoder::new(),
            partial_frame: vec![],
        },
        TTYStream {
            in_queue: iq_rx,
            out_queue: oq_tx,
            close,
        }
    )
}

struct Info {
    pub compressed_len: u32,
    pub decompressed_len: u32,

    pub compressed_crc: u32,
    pub decompressed_crc: u32,

    chunk_size: usize,
}

struct Uploader<'a> {
    args: &'a Args,
    tty: &'a mut TTYStream,
    info: Info,
    compressed: Vec<u8>,

    num_compressed_chunks: usize,
    progress_bar: indicatif::ProgressBar,
}

impl<'a> Uploader<'a> {
    pub fn new(args: &'a Args, tty: &'a mut TTYStream) -> Uploader<'a> {
        let uncompressed = std::fs::read(&args.bin_file)
            .expect(format!("couldn't open {}", args.bin_file.display()).as_str());
        let crc = crc32fast::hash(&uncompressed);
        let info = Info {
            compressed_len: uncompressed.len() as u32,
            decompressed_len: uncompressed.len() as u32,

            compressed_crc: crc,
            decompressed_crc: crc,
            chunk_size: 0,
        };
        Uploader {
            args,
            tty,
            info,
            compressed: uncompressed,
            num_compressed_chunks: 0,
            progress_bar: ProgressBar::new_spinner(),
        }
    }
}

impl<'a> Uploader<'a> {
    pub fn drive(&mut self) -> Result<()> {
        log::info!("[host:v1]: waiting for device to commence upload process");

        loop {
            if let Some((typ, msg)) = self.tty.try_recv() {
                match typ {
                    theseus_common::theseus::handshake::MSG_ALLOWED_CONFIGS => {
                        // ignore, remnant AllowedConfigs
                        log::warn!("[host:v1]: Ignoring leftover Handshake/AllowedConfigs");
                    }
                    v1::MSG_REQUEST_PROGRAM_INFO => {
                        let msg: device::RequestProgramInfo = match postcard::from_bytes(&msg) {
                            Ok(x) => x,
                            Err(e) => {
                                log::error!("[host:v1]: Deserialization error (RequestProgramInfo): {e} on bytes: {}, ignoring.", hexify(&msg));
                                continue;
                            }
                        };
                        self.dispatch_request_program_info(&msg);
                    }
                    v1::MSG_REQUEST_PROGRAM => {
                        let msg: device::RequestProgram = match postcard::from_bytes(&msg) {
                            Ok(x) => x,
                            Err(e) => {
                                log::error!("[host:v1]: Deserialization error (RequestProgram): {e} on bytes: {}, ignoring.", hexify(&msg));
                                continue;
                            }
                        };
                        self.dispatch_request_program(&msg);
                    }
                    v1::MSG_REQUEST_CHUNK => {
                        let msg: device::RequestChunk = match postcard::from_bytes(&msg) {
                            Ok(x) => x,
                            Err(e) => {
                                log::error!("[host:v1]: Deserialization error (RequestChunk): {e} on bytes: {}, ignoring.", hexify(&msg));
                                continue;
                            }
                        };
                        self.dispatch_chunk(&msg);
                    }
                    v1::MSG_BOOTING => {
                        log::info!("[host:v1]: Device is booting...");
                        return Ok(());
                    }
                    t => {
                        log::error!("[host:v1]: Unrecognized message type: {t}, ignoring.");
                    }
                }
            }
        }
    }

    pub fn dispatch_request_program_info(&mut self, _rpi: &device::RequestProgramInfo) {
        log::info!("[host:v1]: Received RequestProgramInfo");
        let msg = &host::ProgramInfo {
            load_at_addr: self.args.address,
            compressed_len: self.info.compressed_len,
            decompressed_len: self.info.decompressed_len,
            compressed_crc: self.info.compressed_crc,
            decompressed_crc: self.info.decompressed_crc,
        };
        if let Err(e) = self.tty.send(msg) {
            log::error!("[host:v1]: Failed to send {msg:?}: {e}, continuing.");
        }
    }

    pub fn dispatch_request_program(&mut self, rp: &device::RequestProgram) {
        log::info!("[host:v1]: Received RequestProgram");
        let compressed_crc_ok = rp.verify_compressed_crc == self.info.compressed_crc;
        let decompressed_crc_ok = rp.verify_decompressed_crc == self.info.decompressed_crc;
        if !compressed_crc_ok {
            log::error!("[host:v1]: Compressed CRC mismatch: expected {} received {}", self.info.compressed_crc, rp.verify_compressed_crc);
        }
        if !decompressed_crc_ok {
            log::error!("[host:v1]: Decompressed CRC mismatch: expected {} received {}", self.info.decompressed_crc, rp.verify_decompressed_crc);
        }
        if compressed_crc_ok && decompressed_crc_ok {
            self.info.chunk_size = rp.chunk_size as usize;
            self.num_compressed_chunks = (self.info.compressed_len as usize + self.info.chunk_size - 1) / self.info.chunk_size;
            self.progress_bar = ProgressBar::new(self.info.compressed_len as u64);
            self.progress_bar.set_style(ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:60.cyan/blue} [{bytes:}/{total_bytes}] {bytes_per_sec}"
            ).unwrap());
            let msg = &host::ProgramReady;
            if let Err(e) = self.tty.send(msg) {
                log::error!("[host:v1]: Failed to send {msg:?}: {e}, continuing.");
            }
        }
    }

    pub fn dispatch_chunk(&mut self, rc: &device::RequestChunk) {
        let chunk_begin = rc.chunk_no as usize * self.info.chunk_size;
        let chunk_end = (chunk_begin + self.info.chunk_size).min(self.compressed.len());

        self.progress_bar.update(|s| {
            s.set_pos(chunk_end as u64)
        });

        let msg = &host::Chunk {
            chunk_no: rc.chunk_no,
            data: &self.compressed[chunk_begin..chunk_end],
        };
        if let Err(e) = self.tty.send(msg) {
            log::error!("[host:v1]: Failed to send {msg:?}: {e}, continuing.");
        }
    }
}

fn v1_upload(args: &Args, tty: &mut TTYStream) -> Result<()> {
    Uploader::new(args, tty).drive()
}

pub fn run(
    args: &Args,
    tty: &mut TTY,
) -> Result<()> {
    let (mut driver, mut stream) = split(tty);
    let r = std::thread::scope(|scope| {
        let jh = scope.spawn(|| { driver.drive() });

        let r = match v1_upload(args, &mut stream) {
            Ok(_) => true,
            Err(e) => {
                log::error!("[host:v1]: Upload failed: {e}");
                false
            }
        };

        stream.close.store(true, Ordering::SeqCst);

        jh.join();

        r
    });

    tty.set_baud_rate(INITIAL_BAUD_RATE);
    log::trace!("[host:v1]: Switching to echo mode");

    if !r {
        log::error!("[host:v1]: Aborting...");
        process::exit(1);
    }
    Ok(())
}