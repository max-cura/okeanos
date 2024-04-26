use bcm2835_lpa::UART1;
use theseus_common::su_boot;
use super::uart1;

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
        // [PRINT_STRING, len(u32), data]
        const PRINT_STRING : u32 = su_boot::Command::PrintString as u32;
        uart1::uart1_write32(self.inner, PRINT_STRING);
        uart1::uart1_write32(self.inner, s.len() as u32);
        uart1::uart1_write_bytes(self.inner, s.as_bytes());
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

impl<const N: usize> TinyBuf<N> {
    const fn new() -> Self {
        Self { inner: [0; N], curs: 0, truncated: false }
    }
    pub fn clear(&mut self) {
        self.inner.iter_mut().for_each(|x| *x = 0);
        self.curs = 0;
        self.truncated = false;
    }
}

impl<const N: usize> Default for TinyBuf<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> core::fmt::Write for TinyBuf<N> {
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

pub static mut BOOT_UMSG_BUF : TinyBuf<0x4000> = TinyBuf::new();

#[macro_export]
macro_rules! legacy_print_string_blocking {
    ($out:expr, $($arg:tt)*) => {
        {
            // let mut buf : $crate::fmt::TinyBuf<1000> = Default::default();
            // TODO: fix this - currently will truncate if buffer is too smaller
            //       I think the fix will be a little complicated so I'm putting it off for now but
            //       the basic idea would be create a local fmt::Write object that pipes to UART1
            //       and basically does what the Write impl for UartWrite is doing rn, except for
            //       the string as a whole and not the pieces of a string, which was kind of a head
            //       empty moment for me
            #[allow(unused_imports)]
            use core::fmt::Write as _;
            {
                let mut tmp = $crate::legacy::fmt::UartWrite::new($out);
                let bub = unsafe { core::mem::transmute::<
                    *mut $crate::legacy::fmt::TinyBuf<0x4000>,
                    &mut $crate::legacy::fmt::TinyBuf<0x4000>>(
                        core::ptr::addr_of_mut!($crate::legacy::fmt::BOOT_UMSG_BUF)) };
                bub.clear();
                let _ = ::core::write!(bub,$($arg)*);
                let _ = tmp.write_str(bub.as_str());
            }
            // let _ = ::core::write!($($arg)*);
        }
    }
}