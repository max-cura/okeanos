use crate::arm1176::__dsb;
use theseus_common::cobs::{BufferedEncoder, EncodeState, PacketEncoder};
use theseus_common::theseus::MessageClass;

/// Circular buffer with FIFO semantics. Overlong writes will be truncated.
#[derive(Debug)]
pub struct TransmissionBuffer {
    underlying_storage: &'static mut [u8],
    circle_begin: usize,
    circle_end: usize,
    /// Amount of data in active use by the buffer.
    circle_len: usize,
}

impl TransmissionBuffer {
    pub fn new(underlying_storage: &'static mut [u8]) -> Self {
        Self {
            underlying_storage,
            circle_begin: 0,
            circle_end: 0,
            circle_len: 0,
        }
    }

    // Simple mutating utility methods -------------------------------------------------------------

    pub fn clear(&mut self) {
        self.underlying_storage.iter_mut().for_each(|x| *x = 0);
        self.circle_begin = 0;
        self.circle_end = 0;
        self.circle_len = 0;
    }

    // Simple read-only utility methods ------------------------------------------------------------

    pub fn is_empty(&self) -> bool {
        self.circle_len == 0
    }

    pub fn remaining_space(&self) -> usize {
        self.underlying_storage.len() - self.circle_len
    }

    // Internals methods ---------------------------------------------------------------------------

    fn _wrapped_add(&self, a: usize, b: usize) -> (usize, bool) {
        // check: i+j<self.underlying_buffer.len()
        // ASSUME: i+j<usize::MAX since usize::MAX is more memory than we have on the Pi Zero
        let i = (a + b) % self.underlying_storage.len();
        (i, i < (a + b))
    }

    /// Write `byte` to the buffer at `offset`, returning the index of the byte immediately
    /// following the one at `offset`.
    fn _push_byte_at_unchecked(&mut self, offset: usize, byte: u8) -> usize {
        self.underlying_storage[offset] = byte;
        self._wrapped_add(offset, 1).0
    }

    /// Write `bytes` into the buffer at offset `offset`, wrapping if necessary. Will not change
    /// `circle_begin`, `circle_end`, or `circle_len`.
    fn _write_bytes_at_unchecked(
        &mut self,
        offset: usize,
        bytes: impl IntoIterator<Item = u8>,
    ) -> usize {
        let mut cursor = offset;
        for byte in bytes.into_iter() {
            cursor = self._push_byte_at_unchecked(cursor, byte);
        }
        cursor
    }

    // Methods for extending the buffer ------------------------------------------------------------

    pub fn push_byte(&mut self, byte: u8) -> bool {
        if self.circle_len == self.underlying_storage.len() {
            // full
            false
        } else {
            self.circle_end = self._push_byte_at_unchecked(self.circle_end, byte);
            // self.underlying_storage[self.circle_end] = byte;
            // self.circle_end += 1;
            self.circle_len += 1;

            true
        }
    }

    pub fn extend_from_slice(&mut self, src: &[u8]) -> bool {
        // LOGIC: circle_len <= storage.len
        if src.len() <= self.remaining_space() {
            for &b in src.iter() {
                // invariants: circle_end and circle_len get written properly by push byte, so this
                // is invariant-safe
                self.push_byte(b);
            }
            true
        } else {
            false
        }
    }

    pub fn reserve(&mut self, n_bytes: usize) -> Option<usize> {
        (n_bytes <= self.remaining_space()).then(|| {
            let v = self.circle_end;
            self.circle_end = self._wrapped_add(self.circle_end, n_bytes).0;
            self.circle_len += n_bytes;
            v
        })
    }

    // Methods for removing bytes at circle_begin --------------------------------------------------

    pub fn shift_byte(&mut self) -> Option<u8> {
        (self.circle_len > 0).then(|| {
            let b = self.underlying_storage[self.circle_begin];
            self.underlying_storage[self.circle_begin] = 0;
            self.circle_begin = self._wrapped_add(self.circle_begin, 1).0;
            self.circle_len -= 1;
            b
        })
    }
}

// Support for checkpointing, to allow rollback of buffer edits if a packet would fully overflow

#[derive(Debug, Copy, Clone)]
struct TransmissionBufferCheckpoint {
    circle_begin: usize,
    circle_end: usize,
    circle_len: usize,
}

impl TransmissionBuffer {
    fn checkpoint(&self) -> TransmissionBufferCheckpoint {
        TransmissionBufferCheckpoint {
            circle_begin: self.circle_begin,
            circle_end: self.circle_end,
            circle_len: self.circle_len,
        }
    }

    fn bytes_since_checkpoint(&self, cp: TransmissionBufferCheckpoint) -> usize {
        let cp_end = cp.circle_end;
        if self.circle_end < cp_end {
            (self.underlying_storage.len() - cp_end) + self.circle_end
        } else {
            self.circle_end - cp_end
        }
    }

    fn restore(&mut self, from: TransmissionBufferCheckpoint) {
        let TransmissionBufferCheckpoint {
            circle_begin,
            circle_end,
            circle_len,
        } = from;
        self._write_bytes_at_unchecked(
            circle_end,
            core::iter::repeat_n(0, self.bytes_since_checkpoint(from)),
        );
        // self.underlying_storage[self.circle_end..circle_end].iter_mut().for_each(|x| *x = 0);
        self.circle_begin = circle_begin;
        self.circle_end = circle_end;
        self.circle_len = circle_len;
    }
}

// Support for theseus messages

pub struct FrameSink {
    transmission_buffer: TransmissionBuffer,
    cobs_encoder: BufferedEncoder<'static>,
    px_buffer: &'static mut [u8],
}

impl FrameSink {
    pub fn new(
        transmission_buffer: TransmissionBuffer,
        cobs_encoder: BufferedEncoder<'static>,
        px_buffer: &'static mut [u8],
    ) -> Self {
        Self {
            transmission_buffer,
            cobs_encoder,
            px_buffer,
        }
    }

    pub fn _flush_to_fifo(&mut self, uart: &bcm2835_lpa::UART1) {
        __dsb();
        while let Some(b) = self.transmission_buffer.shift_byte() {
            while uart.stat().read().tx_ready().bit_is_clear() {}
            uart.io().write(|w| unsafe { w.data().bits(b) })
        }
        __dsb();
    }

    pub fn _buffer(&self) -> &TransmissionBuffer {
        &self.transmission_buffer
    }
    pub fn _buffer_mut(&mut self) -> &mut TransmissionBuffer {
        &mut self.transmission_buffer
    }
    // pub fn _send_raw_bytes(&mut self, bytes: &[u8]) {
    //     self.transmission_buffer.extend_from_slice(bytes);
    // }
    //
    // pub fn _is_empty(&self) -> bool {
    //     self.transmission_buffer.is_empty()
    // }
}

// the problem is: i call send or print_rpc! on FrameSink
// i want a FrameWriter, so I call <FrameSink as FrameWrite>::begin_frame
// the FrameWriter

pub trait FrameWrite {
    fn begin_frame(&mut self) -> FrameWriter;
}

impl FrameWrite for FrameSink {
    // 'b: 'a means that if a concrete lifetime 'z is a valid argument for 'b, it is also a valid
    // argument for 'a, but the reverse doesn't necessarily hold. In essence, 'b is a subset of 'a.
    fn begin_frame(&mut self) -> FrameWriter {
        FrameWriter::new(&mut self.transmission_buffer, self.cobs_encoder.packet())
    }
}

pub struct FrameWriter<'a> {
    transmission_buffer: &'a mut TransmissionBuffer,
    cobs_encoder: PacketEncoder<'a>,

    checkpoint: TransmissionBufferCheckpoint,
    len_offset: usize,
    ok: bool,
    hasher: crc32fast::Hasher,
}

impl<'a> FrameWriter<'a> {
    pub fn new(
        transmission_buffer: &'a mut TransmissionBuffer,
        cobs_encoder: PacketEncoder<'a>,
    ) -> Self {
        let checkpoint = transmission_buffer.checkpoint();
        static PREAMBLE: &[u8; 4] = &[0x55, 0x55, 0x55, 0x5e];
        let ok = transmission_buffer.extend_from_slice(PREAMBLE);
        let (len_offset, ok) = if ok {
            let lo = transmission_buffer.reserve(4);
            (lo.unwrap_or(0), lo.is_some())
        } else {
            (0, false)
        };
        Self {
            transmission_buffer,
            cobs_encoder,
            checkpoint,
            len_offset,
            ok,
            hasher: crc32fast::Hasher::new(),
        }
    }
    pub fn add_bytes(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
        self._add_bytes_unhashed(bytes);
    }
    fn _add_bytes_unhashed(&mut self, bytes: &[u8]) {
        if !self.ok {
            return;
        }
        for byte in bytes.iter().copied() {
            if !self.ok {
                return;
            }
            match self.cobs_encoder.add_byte(byte) {
                EncodeState::Buf(buf) => {
                    if self.ok {
                        self.ok = self.transmission_buffer.extend_from_slice(buf);
                    }
                }
                EncodeState::Pass => {}
            }
        }
    }
    pub fn write32(&mut self, x: u32) {
        self.add_bytes(&x.to_le_bytes());
    }
    pub fn finalize(mut self) -> bool {
        if self.ok {
            let hasher = core::mem::replace(&mut self.hasher, crc32fast::Hasher::new());
            // crc32 should be cobs-encoded, so run it through add_bytes
            self._add_bytes_unhashed(&hasher.finalize().to_le_bytes());
        }
        if self.ok {
            // finalize the packet, at last
            let buf = self.cobs_encoder.finish();
            self.ok = self.transmission_buffer.extend_from_slice(buf);
        }
        if !self.ok {
            self.transmission_buffer.restore(self.checkpoint);
        } else {
            self.transmission_buffer._write_bytes_at_unchecked(
                self.len_offset,
                theseus_common::theseus::len::encode_len(
                    self.transmission_buffer
                        .bytes_since_checkpoint(self.checkpoint)
                        - 8,
                ),
            );
        }
        self.ok
    }
    pub fn abort(self) {
        self.transmission_buffer.restore(self.checkpoint);
    }
}

impl<'a> core::fmt::Write for FrameWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.add_bytes(s.as_bytes());
        Ok(())
    }
}

#[macro_export]
macro_rules! print_rpc {
($w:expr, $($args:tt)*) => {
    #[allow(unused_imports)]
    use ::core::fmt::Write as _;
    #[allow(unused_imports)]
    use $crate::reactor::txbuf::FrameWrite as _;
    let mut fw = $w.begin_frame();
    fw.write32(::theseus_common::theseus::MSG_PRINT_STRING);
    let _ = ::core::write!(fw, $($args)*);
    fw.finalize();
}
}

impl FrameSink {
    pub fn send<T: MessageClass + serde::Serialize>(
        &mut self,
        msg: &T,
    ) -> Result<bool, postcard::Error> {
        let mut fw = FrameWriter::new(&mut self.transmission_buffer, self.cobs_encoder.packet());
        let mut nbuf = [0; 8];
        // message type is a varint
        let x = postcard::to_slice(&T::MSG_TYPE, &mut nbuf)
            .map(|x| &x[..])
            .unwrap_or(&[0]);
        fw.add_bytes(x);
        let r = match postcard::to_slice(msg, self.px_buffer) {
            Ok(buf) => {
                fw.add_bytes(buf);
                Ok(fw.finalize())
            }
            Err(e) => {
                // don't finalize if we failed to encode
                fw.abort();
                Err(e)
            }
        };
        r
    }
}

pub fn send<T: MessageClass + serde::Serialize>(
    fs: &mut FrameSink,
    msg: &T,
) -> Result<bool, postcard::Error> {
    fs.send(msg)
}

// Support for legacy::print_string!()

pub mod legacy_compat {
    use crate::reactor::txbuf::{TransmissionBuffer, TransmissionBufferCheckpoint};

    pub trait LegacyPrintStringWriter {
        fn legacy_writer(&mut self) -> LegacyTxWriter;
    }

    impl LegacyPrintStringWriter for super::FrameSink {
        fn legacy_writer(&mut self) -> LegacyTxWriter {
            LegacyTxWriter::new(&mut self.transmission_buffer)
        }
    }

    pub struct LegacyTxWriter<'a> {
        transmission_buffer: &'a mut TransmissionBuffer,
        checkpoint: TransmissionBufferCheckpoint,
        len_offset: usize,
        ok: bool,
    }

    impl<'a> LegacyTxWriter<'a> {
        pub fn new(transmission_buffer: &'a mut TransmissionBuffer) -> Self {
            let checkpoint = transmission_buffer.checkpoint();
            static PRINT_STRING: &[u8; 4] = &[0xee, 0xee, 0xdd, 0xdd];
            let mut ok = transmission_buffer.extend_from_slice(PRINT_STRING);
            let len_offset = if ok {
                let len_offset = transmission_buffer.reserve(4);
                ok = len_offset.is_some();
                len_offset.unwrap_or(0)
            } else {
                0
            };
            Self {
                transmission_buffer,
                checkpoint,
                len_offset,
                ok,
            }
        }
        pub fn finalize(self) -> bool {
            if !self.ok {
                self.transmission_buffer.restore(self.checkpoint);
            } else {
                self.transmission_buffer._write_bytes_at_unchecked(
                    self.len_offset,
                    (self
                        .transmission_buffer
                        .bytes_since_checkpoint(self.checkpoint) as u32
                        - 8)
                    .to_le_bytes(),
                );
            }
            self.ok
        }
    }

    impl<'a> core::fmt::Write for LegacyTxWriter<'a> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            if self.ok {
                self.ok = self.transmission_buffer.extend_from_slice(s.as_bytes());
            }
            Ok(())
        }
    }
}

#[macro_export]
macro_rules! legacy_print_string {
($w:expr, $($s:tt)*) => {
        // $w is expected to be transmission buffer
        {
        #[allow(unused_imports)]
        use core::fmt::Write as _;
        use $crate::reactor::txbuf::legacy_compat::LegacyPrintStringWriter as _;
        let mut writer = $w.legacy_writer();
        let _ = core::write!(
            writer,
            $($s)*
        );
        writer.finalize();
        }
    }
}
