use bcm2835_lpa::UART1;
use crate::boot::uart1;

pub struct Uart1<'a> {
    inner: &'a UART1,
}

impl<'a> Uart1<'a> {
    pub fn new(uart1: &'a UART1) -> Self {
        Self { inner: uart1 }
    }
    pub fn flush(&mut self) {
        uart1::uart1_flush_tx(self.inner);
    }
}

impl<'a> core::fmt::Write for Uart1<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        uart1::uart1_write_bytes(self.inner, s.as_bytes());
        Ok(())
    }
}

#[macro_export]
macro_rules! uprintln {
    ($($arg:tt)*) => {
        { let _ = ::core::writeln!($($arg)*); }
    };
}