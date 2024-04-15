use core::fmt::Write;
use bcm2835_lpa::UART1;
use theseus_common::su_boot;
use theseus_common::theseus::v1;
use theseus_common::theseus::v1::MESSAGE_PRECURSOR;
use crate::{IN_THESEUS, uart1};
use crate::cobs::EncodeState;

pub struct UartWrite<'a> {
    inner: &'a UART1,
}

impl<'a> UartWrite<'a> {
    pub fn new(uart1: &'a UART1) -> Self {
        Self { inner: uart1 }
    }
    pub fn flush(&mut self) {
        uart1::uart1_flush_tx(self.inner);
    }
}

impl<'a> core::fmt::Write for UartWrite<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if unsafe { IN_THESEUS } {
            // PrintMessageRPC in 254-byte chunks
            let mut enc = crate::cobs::BufferedEncoder::new();
            let mut begin = 0;
            loop {
                let e = (begin + 254).min(s.len());
                // Heisenbug note!
                //  when writing kernel.img to SD and booting from that, this would do a panic
                //  infini-loop: I had forgotten to break from the loop, and begin was overflowing
                //  past the end of the buffer.
                // The reason I didn't notice this before was because I hadn't actually yet
                //  implemented the THESEUS part of the protocol, so IN_THESEUS was always false and
                //  this never ran.
                // But then I pulled up with this kernel.img.
                // Since IN_THESEUS is initialized to false, rustc put it in .bss, which was not
                //  getting zeroed properly-that's my best guess, at least.
                let window = &s[begin..e];

                // totally overkill but eh
                let mut encode_buf: [u8; 256+128] = [0; 256+128];
                let data = postcard::to_slice(&v1::MessageContent::PrintMessageRPC {
                    message: window.as_bytes(),
                }, &mut encode_buf).unwrap();

                let mut crc = crc32fast::Hasher::new();
                uart1::uart1_write32(self.inner, MESSAGE_PRECURSOR);
                let mut p = enc.packet();
                for &byte in &data[..] {
                    match p.add_byte(byte) {
                        EncodeState::Buf(buf) => {
                            uart1::uart1_write_bytes(self.inner, buf);
                            (&mut crc).update(buf);
                        }
                        EncodeState::Pass => {}
                    }
                }
                uart1::uart1_write_bytes(self.inner, p.finish());
                uart1::uart1_write32(self.inner, crc.finalize());

                begin = e;
                if e >= s.len() {
                    break
                }
            }
        } else {
            // [PRINT_STRING, len(32), data]
            const PRINT_STRING : u32 = su_boot::Command::PrintString as u32;
            uart1::uart1_write32(self.inner, PRINT_STRING);
            uart1::uart1_write32(self.inner, s.len() as u32);
            uart1::uart1_write_bytes(self.inner, s.as_bytes());
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TinyBuf<const N: usize> {
    inner: [u8; N],
    curs: usize,
    truncated: bool,
}
impl<const N: usize> TinyBuf<N> {
    pub fn as_str(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.inner[..self.curs]) }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.inner[..self.curs]
    }
}

impl<const N: usize> Default for TinyBuf<N> {
    fn default() -> Self {
        Self { inner: [0; N], curs: 0, truncated: false }
    }
}

impl<const N: usize> Write for TinyBuf<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        const TRUNCATION_NOTICE : &str = "<truncated>";

        if self.truncated {
            return Ok(())
        }

        let mut buf : [u8; 4] = [0; 4];
        for c in s.chars() {
            let enc = c.encode_utf8(&mut buf);
            if (self.curs + enc.len()) >= /* TODO: inline_const */ /* const */ { N - TRUNCATION_NOTICE.len() } {
                self.truncated = true;
                self.inner[self.curs..(self.curs + TRUNCATION_NOTICE.len())]
                    .copy_from_slice(TRUNCATION_NOTICE.as_bytes());
                self.curs += TRUNCATION_NOTICE.len();

                return Ok(())
            } else {
                self.inner[self.curs..(self.curs + enc.len())].copy_from_slice(enc.as_bytes());
                self.curs += enc.len();
            }
        }

        Ok(())
    }
}

#[macro_export]
macro_rules! boot_umsg {
    ($out:tt, $($arg:tt)*) => {
        {
            let mut buf : $crate::fmt::TinyBuf<1000> = Default::default();
            // TODO: fix this - currently will truncate if buffer is too smaller
            //       I think the fix will be a little complicated so I'm putting it off for now but
            //       the basic idea would be create a local fmt::Write object that pipes to UART1
            //       and basically does what the Write impl for UartWrite is doing rn, except for
            //       the string as a whole and not the pieces of a string, which was kind of a head
            //       empty moment for me
            {
                let _ = ::core::write!(&mut buf, $($arg)*);
                let _ = $out.write_str(buf.as_str());
            }
            // let _ = ::core::write!($($arg)*);
        }
    };
}