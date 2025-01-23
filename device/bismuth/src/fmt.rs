use bcm2835_lpa::UART1;
use quartz::arch::arm1176::__dsb;

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
        __dsb();
        for &b in s.as_bytes() {
            while !self.inner.stat().read().tx_ready().bit_is_set() {}
            self.inner.io().write(|w| unsafe { w.data().bits(b) });
        }
        __dsb();
        Ok(())
    }
}

#[macro_export]
macro_rules! uart1_println {
    ($out:expr, $($arg:tt)*) => {
        {
            // TODO: fix this - currently will truncate if buffer is too smaller
            //       I think the fix will be a little complicated so I'm putting it off for now but
            //       the basic idea would be create a local fmt::Write object that pipes to UART1
            //       and basically does what the Write impl for UartWrite is doing rn, except for
            //       the string as a whole and not the pieces of a string, which was kind of a head
            //       empty moment for me
            {
                #[allow(unused_imports)]
                use core::fmt::Write as _;
                let mut proxy = $crate::fmt::Uart1WriteProxy::new($out);
                // let bub = unsafe { &mut *$crate::legacy::fmt::BOOT_UMSG_BUF.0.get() };
                // bub.clear();
                let _ = ::core::writeln!(proxy, $($arg)*);
                // let _ = tmp.write_str(bub.as_str());
                quartz::device::bcm2835::mini_uart::mini_uart1_flush_tx($out);
            }
        }
    }
}
