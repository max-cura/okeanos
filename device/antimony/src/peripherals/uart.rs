use crate::APB_FREQ;
use crate::peripherals::{Pin, PinBound, PinFunc, PinId};
use core::fmt::{Debug, Formatter, Write};
use core::marker::ConstParamTy;
use core::ops::Deref;
use d1_pac::uart::RegisterBlock;
use d1_pac::{UART0, UART1, UART2, UART3};
use thiserror::Error;

#[derive(Eq, PartialEq, ConstParamTy)]
pub enum Mode {
    Direct,
}

#[derive(Debug, Clone, Error)]
pub enum Error {}

pub trait UartDevice<const TXP: PinId, const RXP: PinId>: Deref<Target = RegisterBlock> {
    fn new(txp: Pin<TXP>, rxp: Pin<RXP>) -> Self
    where
        Self: Sized;
}

pub struct UartDev<T, const TXP: PinFunc, const RXP: PinFunc> {
    device: T,
    #[allow(dead_code)]
    tx: PinBound<TXP>,
    #[allow(dead_code)]
    rx: PinBound<RXP>,
}
macro define_uart($uart:ident, $tx:ident as $tsel:ident, $rx:ident as $rsel:ident) {
    impl UartDevice<{ PinId::$tx }, { PinId::$rx }>
        for UartDev<
            $uart,
            { PinFunc::$tx($crate::peripherals::pin::$tx::$tsel) },
            { PinFunc::$rx($crate::peripherals::pin::$rx::$rsel) },
        >
    {
        fn new(txp: Pin<{ PinId::$tx }>, rxp: Pin<{ PinId::$rx }>) -> Self
        where
            Self: Sized,
        {
            let tx = txp.select::<{ $crate::peripherals::pin::$tx::$tsel }>();
            let rx = rxp.select::<{ $crate::peripherals::pin::$rx::$rsel }>();
            UartDev {
                device: unsafe { $uart::steal() },
                tx,
                rx,
            }
        }
    }
}

define_uart!(UART0, PB0 as UART0_TX, PB1 as UART0_RX);
define_uart!(UART2, PB0 as UART2_TX, PB1 as UART2_RX);
define_uart!(UART0, PB8 as UART0_TX, PB9 as UART0_RX);
define_uart!(UART1, PB8 as UART1_TX, PB9 as UART1_RX);
define_uart!(UART2, PD21 as UART2_TX, PD22 as UART2_RX);
define_uart!(UART3, PD10 as UART3_TX, PD11 as UART3_RX);

impl<T: Deref<Target = RegisterBlock>, const TXP: PinFunc, const RXP: PinFunc> Deref
    for UartDev<T, TXP, RXP>
{
    type Target = RegisterBlock;
    fn deref(&self) -> &Self::Target {
        &self.device
    }
}

// Uart

pub struct Uart<U: Deref<Target = RegisterBlock>, const M: Mode> {
    device: U,
}
impl<U: Deref<Target = RegisterBlock>, const M: Mode> Debug for Uart<U, M> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Uart").finish()
    }
}

// Direct mode

impl<U: Deref<Target = RegisterBlock>> Uart<U, { Mode::Direct }> {
    /// Begin operation in 8n1 mode
    pub fn new(device: U, baud_rate: u32) -> Result<Self, Error> {
        device.ier().write(|w| unsafe { w.bits(0) });
        device.fcr().write(|w| unsafe { w.bits(0) });
        device.mcr().write(|w| unsafe { w.bits(0) });
        device.lcr().write(|w| unsafe { w.bits(0) });

        let prescaler = APB_FREQ / 16 / baud_rate;
        let prescaler_low = ((prescaler & 0x0000_0000_0000_ffff) >> 0) as u8;
        let prescaler_high = ((prescaler & 0x0000_0000_ffff_0000) >> 8) as u8;

        device.lcr().write(|w| w.dlab().divisor_latch());
        device.dll().write(|w| w.dll().variant(prescaler_low)); // 24  * 1000 * 1000 / 16 / 115200
        device.dlh().write(|w| w.dlh().variant(prescaler_high));
        device.lcr().write(|w| w.dlab().rx_buffer());
        device.fcr().write(|w| w.fifoe().set_bit());

        device
            .lcr()
            .write(|w| w.pen().disabled().stop().one().dls().eight().eps().odd());

        Ok(Self { device })
    }
}
impl<U: Deref<Target = RegisterBlock>> Write for Uart<U, { Mode::Direct }> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            while self.device.usr().read().tfnf().bit_is_clear() {}
            self.device.thr().write(|w| w.thr().variant(byte));
        }
        while self.device.lsr().read().temt().bit_is_clear() {}
        Ok(())
    }
}
