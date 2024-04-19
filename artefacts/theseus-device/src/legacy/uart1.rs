use bcm2835_lpa::UART1;
use crate::arm1176::__dsb;

pub fn uart1_write8(uart1_device: &UART1, x: u8) {
    __dsb();

    while !uart1_device.stat().read().tx_ready().bit_is_set() {}

    uart1_device.io().write(|w| {
        unsafe { w.data().bits(x) }
    });

    __dsb();
}

pub fn uart1_write_bytes(uart1_device: &UART1, x: &[u8]) {
    for &b in x.iter() {
        uart1_write8(uart1_device, b)
    }
}

pub fn uart1_write32(uart1_device: &UART1, x: u32) {
    uart1_write_bytes(uart1_device, &u32::to_le_bytes(x));
}

pub fn uart1_read32_blocking(uart1_device: &UART1) -> u32 {
    let mut buf = [0;4];
    for i in 0..4 {
        buf[i] = uart1_read8_blocking(uart1_device);
    }
    u32::from_le_bytes(buf)
}

pub fn uart1_read8_nb(
    uart1_device: &UART1,
) -> Option<u8> {
    __dsb();

    let r = if uart1_device.stat().read().data_ready().bit_is_set() {
        Some(uart1_device.io().read().data().bits())
    } else {
        None
    };

    __dsb();
    r
}

pub fn uart1_read8_blocking(uart1_device: &UART1) -> u8 {
    __dsb();

    while !uart1_device.stat().read().data_ready().bit_is_set() {}

    let b = uart1_device.io().read().data().bits();

    __dsb();

    b
}

pub fn uart1_flush_tx(uart1_device: &UART1) {
    __dsb();

    while !uart1_device.stat().read().tx_empty().bit_is_set() {}

    __dsb();
}
