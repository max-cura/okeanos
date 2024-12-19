use okboot_common::frame::{FrameEncoder, SliceBufferedEncoder};
use okboot_common::PREAMBLE_BYTES;

/// Circular buffer with FIFO semantics. Overlong writes will be truncated.
#[derive(Debug)]
pub struct TransmissionBuffer<'a> {
    underlying_storage: &'a mut [u8],
    circle_begin: usize,
    circle_end: usize,
    /// Amount of data in active use by the buffer.
    circle_len: usize,
}

impl<'a> TransmissionBuffer<'a> {
    pub fn new(underlying_storage: &'a mut [u8]) -> Self {
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

impl<'a> TransmissionBuffer<'a> {
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

pub struct FrameSink<'tb, 'be> {
    transmission_buffer: TransmissionBuffer<'tb>,
    slice_buffered_encoder: SliceBufferedEncoder<'be>,
}
impl FrameSink {
    pub fn new(
        transmission_buffer: TransmissionBuffer,
        slice_buffered_encoder: SliceBufferedEncoder,
    ) -> Self {
        Self {
            transmission_buffer,
            slice_buffered_encoder,
        }
    }
}

pub trait FrameWrite {
    fn write_frame(&mut self) -> FrameWriter;
}
impl<'tb, 'be> FrameWrite for FrameSink<'tb, 'be> {
    fn write_frame<'fs>(&mut self) -> FrameWriter<'fs, 'tb, 'be> {
        FrameWriter::new(
            &mut self.transmission_buffer,
            self.slice_buffered_encoder.frame().unwrap(),
        )
    }
}

pub struct FrameWriter<'tbr, 'tb, 'fe> {
    transmission_buffer: &'tbr mut TransmissionBuffer<'tb>,
    frame_encoder: FrameEncoder<'fe>,

    checkpoint: TransmissionBufferCheckpoint,
    len_offset: usize,
    ok: bool,
    hasher: crc32fast::Hasher,
}
impl<'tbr, 'tb, 'fe> FrameWriter<'tbr, 'tb, 'fe> {
    pub fn new(
        transmission_buffer: &'tbr mut TransmissionBuffer<'tb>,
        frame_encoder: FrameEncoder<'fe>,
    ) -> Self {
        let checkpoint = transmission_buffer.checkpoint();
        let ok = transmission_buffer.extend_from_slice(&PREAMBLE_BYTES);
    }
}
