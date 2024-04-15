use bcm2835_lpa::{SYSTMR, UART1};
use crate::boot_umsg;
use crate::fmt::UartWrite;
use core::fmt::Write;
use crate::delay::delay_millis;

pub mod v1;

pub(crate) fn perform_download(
    uw: &mut UartWrite,
    uart: &UART1,
    st: &SYSTMR
) {
    // we just received MESSAGE_PRECURSOR
    boot_umsg!(uw, "[theseus-device]: received MESSAGE_PRECURSOR");

    // delay_millis(st, 50);

    boot_umsg!(uw, "[theseus-device]: download failed, rebooting");
}