use bcm2835_lpa::{AUX, GPIO, UART1};
use crate::arm1176::__dsb;

const MINI_UART_CLOCK_RATE : u32 = 250_000_000;

/// Calculate a value for the Mini UART clock divider from the desired baud rate.
pub const fn baud_to_clock_divider(
    baud_rate: u32
) -> u16 {
    ((MINI_UART_CLOCK_RATE / (8 * baud_rate)) - 1) as u16
}

pub fn uart1_init(
    gpio: &GPIO,
    aux: &AUX,
    uart: &UART1,
    clock_divider: u16,
) {
    // TODO: check interrupts disabled

    __dsb();

    gpio.gpfsel1().modify(|_, w| {
        w
            .fsel14().txd1()
            .fsel15().rxd1()
    });

    __dsb();

    aux.enables().modify(|_, w| {
        w.uart_1().set_bit()
    });

    __dsb();

    uart.cntl().modify(|_, w| {
        w
            .tx_enable().clear_bit()
            .rx_enable().clear_bit()
    });
    uart.ier().modify(|_, w| {
        // docs are a bit screwy-I also don't completely trust bcm2835-lpa here, either
        // however, we can just w.bits(0) to disable all interrupts so
        unsafe { w.bits(0) }
    });
    uart.iir().modify(|_, w| {
        // names are wrong - functionality on writing is different from
        // functionality on read.
        // writing 11 to bits 3:2 will clear both FIFOs
        w
            .tx_ready().set_bit()
            .data_ready().set_bit()
    });
    uart.baud().write(|w| {
        unsafe { w.bits(clock_divider) }
    });
    uart.lcr().modify(|_, w| {
        w
            .data_size()._8bit()
            // .break_().clear_bit()
            // .dlab().clear_bit()
    });
    uart.mcr().modify(|_, w| {
        w
            .rts().clear_bit()
    });
    uart.cntl().modify(|_, w| {
        w
            .cts_enable().clear_bit()
            .rts_enable().clear_bit()
    });
    uart.cntl().modify(|_, w| {
        w
            .tx_enable().set_bit()
            .rx_enable().set_bit()
    });

    __dsb();
}
