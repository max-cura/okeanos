use crate::arch::arm1176::__dsb;

// Support for checkpointing, to allow rollback of buffer edits if a packet would fully overflow

#[derive(Debug, Copy, Clone)]
struct BufferCheckpoint {
    circle_begin: usize,
    circle_end: usize,
    circle_len: usize,
}

/// Circular buffer with FIFO semantics. Overlong writes will be truncated.
#[derive(Debug)]
pub struct CircularBuffer {
    underlying_storage: &'static mut [u8],
    circle_begin: usize,
    circle_end: usize,
    /// Amount of data in active use by the buffer.
    circle_len: usize,
}

impl CircularBuffer {
    pub(crate) fn view(&self) -> (&[u8], &[u8]) {
        if self.circle_end < self.circle_begin {
            (
                &self.underlying_storage[self.circle_begin..],
                &self.underlying_storage[..self.circle_end],
            )
        } else {
            (
                &self.underlying_storage[self.circle_begin..self.circle_end],
                &[],
            )
        }
    }
}

impl CircularBuffer {
    pub(crate) fn circle(&self) -> (usize, usize, usize) {
        (self.circle_len, self.circle_begin, self.circle_end)
    }
}

impl CircularBuffer {
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

    /// Will not write anything if there is insufficient space.
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

    // Methods for working with checkpoints --------------------------------------------------------

    fn checkpoint(&self) -> BufferCheckpoint {
        BufferCheckpoint {
            circle_begin: self.circle_begin,
            circle_end: self.circle_end,
            circle_len: self.circle_len,
        }
    }

    fn bytes_since_checkpoint(&self, cp: BufferCheckpoint) -> usize {
        let cp_end = cp.circle_end;
        if self.circle_end < cp_end {
            (self.underlying_storage.len() - cp_end) + self.circle_end
        } else {
            self.circle_end - cp_end
        }
    }

    fn restore(&mut self, from: BufferCheckpoint) {
        let BufferCheckpoint {
            circle_begin,
            circle_end,
            circle_len,
        } = from;
        self._write_bytes_at_unchecked(
            circle_end,
            core::iter::repeat_n(0, self.bytes_since_checkpoint(from)),
        );
        self.circle_begin = circle_begin;
        self.circle_end = circle_end;
        self.circle_len = circle_len;
    }

    pub fn _flush_to_uart1_fifo(&mut self, uart: &bcm2835_lpa::UART1) {
        __dsb();
        while let Some(b) = self.shift_byte() {
            while uart.stat().read().tx_ready().bit_is_clear() {}
            uart.io().write(|w| unsafe { w.data().bits(b) })
        }
        __dsb();
    }
}

impl core::fmt::Write for CircularBuffer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.extend_from_slice(s.as_bytes()) {
            Ok(())
        } else {
            Err(core::fmt::Error::default())
        }
    }
}

/// Write to a [`CircularBuffer`]. If the operation fails due to lack of space, any results in the
/// buffer will be rolled back, and it will attempt to write the character '$' to the buffer.
///
/// ```
/// let mut cb : CircularBuffer = /* */;
/// cb_print!(cb, "Hello {}", "world");
/// ```
#[macro_export]
macro_rules! cb_print {
($w:expr, $($args:tt)*) => {
    {
        let w : &mut CircularBuffer = &mut $w;
        #[allow(unused_imports)]
        use ::core::fmt::Write as _;
        let checkpoint = w.checkpoint();
        if ::core::write!(w, $($args)*).is_err() {
            w.restore(checkpoint);
            // try to write '$'
            w.push_byte(b'$');
        }
    }
}
}
pub use cb_print;

/// Write to a [`CircularBuffer`], followed by a newline. If the operation fails due to lack of
/// space, any results in the buffer will be rolled back, and it will attempt to write the character
/// '$' to the buffer.
///
/// ```
/// let mut cb : CircularBuffer = /* */;
/// cb_println!(cb, "Hello {}", "world");
/// ```
#[macro_export]
macro_rules! cb_println {
($w:expr, $($args:tt)*) => {
    {
        let w : &mut CircularBuffer = AsMut::as_mut($w);
        #[allow(unused_imports)]
        use ::core::fmt::Write as _;
        let checkpoint = w.checkpoint();
        if ::core::writeln!(w, $($args)*).is_err() {
            w.restore(checkpoint);
            // try to write '$'
            w.push_byte(b'$');
        }
    }
}
}
pub use cb_println;
