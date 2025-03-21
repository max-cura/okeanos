use bcm2835_lpa::UART1;
use quartz::arch::arm1176::dsb;

pub struct Uart1WriteProxy<'a> {
    inner: &'a UART1,
}

impl<'a> Uart1WriteProxy<'a> {
    pub fn new(uart1: &'a UART1) -> Self {
        Self { inner: uart1 }
    }
    #[allow(unused)]
    pub fn flush(&mut self) {
        quartz::device::bcm2835::mini_uart::mini_uart1_flush_tx(self.inner);
    }
}

impl<'a> core::fmt::Write for Uart1WriteProxy<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        dsb();
        for &b in s.as_bytes() {
            while !self.inner.stat().read().tx_ready().bit_is_set() {}
            self.inner.io().write(|w| unsafe { w.data().bits(b) });
        }
        dsb();
        Ok(())
    }
}

#[macro_export]
macro_rules! uart1_println {
    ($out:expr, $($arg:tt)*) => {
        {
                #[allow(unused_imports)]
                use ::core::fmt::Write as _;
                let mut proxy = $crate::fmt::Uart1WriteProxy::new($out);
                let _ = ::core::writeln!(proxy, $($arg)*);
                ::quartz::device::bcm2835::mini_uart::mini_uart1_flush_tx($out);
        }
    }
}
