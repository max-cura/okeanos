use core::arch::asm;
use bcm2835_lpa::{AUX, GPIO, UART1};
use crate::data_synchronization_barrier;

// 250MHz
const CLOCK_RATE : u32 = 250_000_000;
pub const fn calculate_baud_register_value(
    baud_rate: u32
) -> u16 {
    // baud_rate = system_clock_freq / 8(value+1)
    // value = (system_clock_freq/8baud_rate - 1)

    // // %0 can generate a fault (divide by zero)
    // if baud_rate == 0 { return None }
    // // we should not have to round!!
    // if (31250000 % baud_rate) != 0 { return None }
    // // must fit in the 16-bit AUX_MU_BAUD_REG
    let value = (CLOCK_RATE / (8 * baud_rate)) - 1;
    // if value > { u16::MAX as u32 } { return None }

    value as u16
}

pub fn uart1_init(
    gpio_device: &GPIO,
    aux_device: &AUX,
    uart1_device: &UART1,
    baud_rate: u32,
) {
    data_synchronization_barrier();

    // select TXD1/RXD1 on GPIO14/15
    gpio_device.gpfsel1().modify(|_, w| {
        w
            .fsel14().txd1()
            .fsel15().rxd1()
    });

    data_synchronization_barrier();

    // set AUXENB.0 on the AUX_ENABLES register
    aux_device.enables().modify(|_, w| {
        w.uart_1().set_bit()
    });

    data_synchronization_barrier();

    // disable xmit/recv
    uart1_device.cntl().modify(|_, w| {
        w
            .tx_enable().clear_bit()
            .rx_enable().clear_bit()
    });
    // disable interrupts
    uart1_device.ier().modify(|_, w| {
        // TODO: make this nicer
        unsafe { w.bits(0) }
    });
    // flush buffers
    uart1_device.iir().modify(|_, w| {
        // names are wrong - functionality on writing is different from
        // functionality on read.
        // writing 11 to bits 3:2 will clear both FIFOs
        w
            .tx_ready().set_bit()
            .data_ready().set_bit()
    });
    // sanity check (currently unwired)
    let aux_mu_stat_reg = uart1_device.stat().read();
    // TODO: we might need to wait for a little bit to make sure that tx/rx can finish the current
    //       symbol if for some reason there is any - I rather doubt this would happen though
    let sane = {
        aux_mu_stat_reg.rx_fifo_level().bits() == 0
        && aux_mu_stat_reg.tx_fifo_level().bits() == 0
        && aux_mu_stat_reg.rx_idle().bit()
        && aux_mu_stat_reg.tx_done().bit()
    };

    // disable interrupts: INTERRUPTS ALREADY DISABLED
    {}

    // TODO: wire
    let _ = sane;
    // set baud rate
    let baud_reg = calculate_baud_register_value(baud_rate);
    uart1_device.baud().write(|w| {
        unsafe { w.bits(baud_reg) }
    });
    // 8-bit mode
    uart1_device.lcr().modify(|_, w| {
        // does in fact set both bits
        w.data_size()._8bit()
    });
    // TODO: do we need this?
    uart1_device.mcr().modify(|_, w| {
        w.rts().clear_bit()
    });
    uart1_device.cntl().modify(|_, w| {
        // disable flow control
        w
            .cts_enable().clear_bit()
            .rts_enable().clear_bit()
    });
    uart1_device.cntl().modify(|_, w| {
        w
            .tx_enable().set_bit()
            .rx_enable().set_bit()
    });

    // DONT ENABLE INTERRUPTS
    {}

    data_synchronization_barrier();
}

pub fn uart1_write8(uart1_device: &UART1, x: u8) {
    data_synchronization_barrier();

    while !uart1_device.stat().read().tx_ready().bit_is_set() {
        unsafe { asm!("nop") }
    }

    uart1_device.io().write(|w| {
        unsafe { w.data().bits(x) }
    });

    data_synchronization_barrier();
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
    data_synchronization_barrier();

    let r = if uart1_device.stat().read().data_ready().bit_is_set() {
        Some(uart1_device.io().read().data().bits())
    } else {
        None
    };

    data_synchronization_barrier();
    r
}

pub fn uart1_read8_blocking(uart1_device: &UART1) -> u8 {
    data_synchronization_barrier();

    while !uart1_device.stat().read().data_ready().bit_is_set() {
        unsafe { asm!("nop") }
    }

    let b = uart1_device.io().read().data().bits();

    data_synchronization_barrier();

    b
}

pub fn uart1_flush_tx(uart1_device: &UART1) {
    data_synchronization_barrier();

    while !uart1_device.stat().read().tx_empty().bit_is_set() {
        unsafe { asm!("nop") }
    }

    data_synchronization_barrier();
}