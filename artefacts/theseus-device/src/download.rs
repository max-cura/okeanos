use core::time::Duration;
use bcm2835_lpa::{SYSTMR, UART1};
use theseus_common::su_boot;
use crate::delay::{delay_micros, delay_millis, st_read, STInstant};
use crate::fmt::UartWrite;
use crate::{boot_umsg, IN_THESEUS, uart1};
use core::fmt::Write;

pub fn download(uart_write: &mut UartWrite, uart: &UART1, st: &SYSTMR) {
    match selector(uart_write, uart, st) {
        SelectorResult::LegacyPutProgInfo => {
            crate::legacy::perform_download(uart_write, uart, st);
        }
        SelectorResult::TheseusMessagePrecursor => {
            unsafe { IN_THESEUS = true };
            boot_umsg!(uart_write, "[theseus-device]: THESEUS protocol not yet implemented");
        }
    }
}

const GET_PROG_INFO : u32 = su_boot::Command::GetProgInfo as u32;

enum SelectorResult {
    LegacyPutProgInfo,
    TheseusMessagePrecursor,
}

fn selector(_uart_write: &mut UartWrite, uart: &UART1, st: &SYSTMR) -> SelectorResult {
    enum S {
        CLR,
        PPI1,
        PPI2,
        PPI3,
        LegacyPutProgInfo,
        THMP1,
        THMP2,
        THMP3,
        TheseusMessagePrecursor,
    }
    let mut marker_state = S::CLR;
    loop {
        uart1::uart1_write32(uart, GET_PROG_INFO);
        // TODO: THESEUS version word
        // uart1::uart1_write32(uart, theseus_common::theseus::TheseusVersion::TheseusV1 as u32);

        let write_time = STInstant::now(st);

        // technically this is noncompliant with the original SU-BOOT protocol which expects 300ms
        // interval, but it should be okay
        while write_time.elapsed(st) < Duration::from_millis(100) {
            let Some(byte) = uart1::uart1_read8_nb(uart) else {
                delay_micros(st, 1);
                continue
            };
            marker_state = match (marker_state, byte) {
                (S::CLR, 0x44) => S::PPI1,
                (S::PPI1, 0x44) => S::PPI2,
                (S::PPI2, 0x33) => S::PPI3,
                (S::PPI3, 0x33) => S::LegacyPutProgInfo,
                (S::CLR, 0x55) => S::THMP1,
                (S::THMP1, 0x77) => S::THMP2,
                (S::THMP2, 0xaa) => S::THMP3,
                (S::THMP3, 0xff) => S::TheseusMessagePrecursor,
                (_, _) => S::CLR,
            };
            match marker_state {
                S::LegacyPutProgInfo => return SelectorResult::LegacyPutProgInfo,
                S::TheseusMessagePrecursor => return SelectorResult::TheseusMessagePrecursor,
                _ => {}
            }
        }
    }
}
