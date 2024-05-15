use theseus_common::theseus::{handshake, MessageTypeType};
use theseus_common::theseus::handshake::device::AllowedConfigs;
use crate::muart::{self, baud_to_clock_divider};
use crate::reactor::{Protocol, ProtocolEnum, ProtocolResult, Reactor, Timeouts, txbuf};
use crate::reactor::txbuf::FrameSink;
use crate::{legacy_print_string, timing};
use crate::reactor::v1::V1;

const SUPPORTED_PROTOCOL_VERSIONS: &[u16] = &[1];
const SUPPORTED_BAUDS: &[u32] = &[
    // UartClock::B115200.to_baud()
    115200, // 270
    230400,
    576000,
    921600,
    1_500_000,
    // 2_000_000,
];

#[derive(Debug)]
enum S {
    ReceivedProbe,
    ReceivedConfig,
}

#[derive(Debug)]
pub struct Handshake {
    state: S,
}
impl Handshake {
    pub fn new() -> Self {
        Self {
            state: S::ReceivedProbe
        }
    }
}
impl Protocol for Handshake {
    fn handle_packet(
        &mut self,
        mtt: MessageTypeType,
        msg_data: &[u8],
        rz: &Reactor,
        fs: &mut FrameSink,
        _timeouts: &mut Timeouts,
    ) -> ProtocolResult {
        match mtt {
            handshake::MSG_PROBE => {
                if !matches!(self.state, S::ReceivedProbe) {
                    legacy_print_string!(fs, "[device]: received Handshake/Probe, expected Handshake/UseConfig");
                    ProtocolResult::Abcon
                } else if let Err(e) = txbuf::send(fs, &handshake::device::AllowedConfigs::new(
                    SUPPORTED_PROTOCOL_VERSIONS,
                    SUPPORTED_BAUDS,
                )) {
                    legacy_print_string!(fs, "[device]: failed to send Handshake/AllowedConfigs: serialization error: {e}");
                    ProtocolResult::Abcon
                } else {
                    legacy_print_string!(fs, "[device]: received Handshake/Probe");
                    self.state = S::ReceivedConfig;
                    match fs.send(&AllowedConfigs::new(SUPPORTED_PROTOCOL_VERSIONS, SUPPORTED_BAUDS)) {
                        Ok(b) => legacy_print_string!(fs, "[device]: sent: {b}"),
                        Err(e) => legacy_print_string!(fs, "[device]: failed to send: {e}"),
                    }
                    ProtocolResult::Continue
                }
            }
            handshake::MSG_USE_CONFIG => {
                legacy_print_string!(fs, "[device]: received Handshake/UseConfig");

                // let blinken = super::Blinken::init(&rz.peri.GPIO);
                //
                // blinken.set(&rz.peri.GPIO, 0);

                if !matches!(self.state, S::ReceivedConfig) {
                    legacy_print_string!(fs, "[device]: received Handshake/UseConfig, expected Handshake/Probe");
                    return ProtocolResult::Abend
                }

                let config : handshake::host::UseConfig = match postcard::from_bytes(msg_data) {
                    Ok(x) => x,
                    Err(e) => {
                        legacy_print_string!(fs, "[device]: failed to receive Handshake/UseConfig: deserialization error: {e}");
                        return ProtocolResult::Abend
                    }
                };
                // config.version config.baud
                // procedure:
                //  1 check that version and baud are valid
                //  2 finish writing out tx_buf (note that we do this ONlY in this one location, so
                //    we can be a bit hacky with it)
                //  3 switch clock through muart
                //  4 start feeding 0x5f, until we receive (continuous) 0x5f
                //  5 continue to feed 0x5f until we stop receiving 0x5f
                //  6 clear out txbuffer, then dump out rxbuffer
                //  7 switch protocol (how?) to V1
                //
                //  4a if we do not receive 0x5f within UNKNOWN ms, ABCON
                //  5a if we do not stop receiving 0x5f within UNKNOWN ms, ABCON
                if !SUPPORTED_PROTOCOL_VERSIONS.contains(&config.version) {
                    legacy_print_string!(fs, "[device]: Handshake/UseConfig: unsupported version number: {}", config.version);
                    return ProtocolResult::Abend
                }
                if !SUPPORTED_BAUDS.contains(&config.baud) {
                    legacy_print_string!(fs, "[device]: Handshake/UseConfig: unsupported baud rate: {}", config.baud);
                    return ProtocolResult::Abend
                }

                let new_clock_divider = baud_to_clock_divider(config.baud);

                legacy_print_string!(fs, "[device]: setting baud rate to: {}Bd, divider={new_clock_divider}#", config.baud);

                let uart = &rz.peri.UART1;
                let systmr = &rz.peri.SYSTMR;

                // let skreeonk_timeout = timeouts::RateRelativeTimeout::from_bytes(0x1000)
                //     .at_baud_8n1(config.baud);
                // time to send 1 byte at new baud + 50%
                // let byte_timeout = timeouts::RateRelativeTimeout::from_bytes(1)
                //     .at_baud_8n1(config.baud * 2 / 3);
                // legacy_print_string!(fs, "[device]: SKREEONK timeout is {skreeonk_timeout:?}, byte timeout is {byte_timeout:?}");

                // flush the transmission buffer into the FIFO
                fs._flush_to_fifo(uart);

                // try to set the clock rate: this will flush the TX FIFO
                if !muart::__uart1_set_clock(uart, new_clock_divider) {
                    legacy_print_string!(fs, "[device]: setting baud rate failed (divider readback failed)");

                    // super::Blinken::init(&rz.peri.GPIO)
                    //     ._6(&rz.peri.GPIO, true);
                    //
                    // loop {
                    //     __dsb();
                    //
                    //     let uart = &rz.peri.UART1;
                    //     if uart.stat().read().tx_ready().bit_is_set() {
                    //         uart.io().write(|w| unsafe { w.data().bits(0x55) });
                    //     }
                    //
                    //     __dsb();
                    // }
                    return ProtocolResult::Abend
                }

                // super::Blinken::init(&rz.peri.GPIO)
                //     ._8(&rz.peri.GPIO, true);
                //
                // loop {
                //     __dsb();
                //
                //     let uart = &rz.peri.UART1;
                //     if uart.stat().read().tx_ready().bit_is_set() {
                //         uart.io().write(|w| unsafe { w.data().bits(0x55) });
                //     }
                //
                //     __dsb();
                // }

                legacy_print_string!(fs, "[device:v{}]: setting baud rate to: {}Bd (divider={new_clock_divider})", config.version, config.baud);

                // ALTERNATE IMPLEMENTATION: wait for 50ms and then continue as normal

                timing::delay_millis(systmr, 50);
                crate::print_rpc!(fs, "[device:v{}]: transitioned baud rate!", config.version);

                ProtocolResult::__SwitchProtocol(match config.version {
                    1 => ProtocolEnum::V1(V1::new(rz, config.baud)),
                    _ => unreachable!("unsupported protocol! should be impossible"),
                })
                //
                // let skreeonk_start = timing::Instant::now(systmr);
                // let mut last_byte = timing::Instant::now(systmr);
                // // feed 0x5f until we receive sequence-8
                // if !skreeonk(
                //     skreeonk_timeout,
                //     byte_timeout,
                //     &skreeonk_start,
                //     &mut last_byte,
                //     systmr,
                //     uart,
                //     0x5f,
                //     0x5f,
                //     8
                // ) {
                //     muart::__uart1_set_clock(uart, INITIAL_BAUD_RATE.to_divider());
                //     legacy_print_string!(fs, "[device]: failed to transition baud rate! received no response to SKREEONK");
                //     return ProtocolResult::Abend
                // }
                //
                // muart::__uart1_clear_fifos(uart);
                // crate::print_rpc!(fs, "[device]: transitioned baud rate!");
            }
            x => {
                legacy_print_string!(fs, "[device]: in Handshake protocol: unexpected message type: {x}");
                ProtocolResult::Abend
            }
        }
    }

    fn heartbeat(
        &mut self,
        _reactor: &Reactor,
        _fs: &mut FrameSink,
        _timeouts: &mut Timeouts,
    ) -> ProtocolResult {
        // do nothing -- all the action happens in handle_packet, since Handshake is the only
        // protocol segment where we're purely reactive
        ProtocolResult::Continue
    }
}

// fn skreeonk(
//     skreeonk_timeout: Duration,
//     byte_timeout: Duration,
//     skreeonk_start: &timing::Instant,
//     last_byte: &mut timing::Instant,
//     systmr: &SYSTMR,
//     uart: &UART1,
//     send: u8,
//     recv: u8,
//     n: usize,
// ) -> bool {
//     let mut seq_no = 0;
//     *last_byte = timing::Instant::now(systmr);
//     'skreeonk: loop {
//         if skreeonk_start.elapsed(systmr) > skreeonk_timeout {
//             break 'skreeonk false
//         }
//         __dsb();
//         let lsr = uart.lsr().read();
//         if lsr.tx_empty().bit_is_set() { // misleading name
//             uart.io().write(|w| unsafe { w.data().bits(send) });
//         }
//         let byte = lsr.data_ready().bit_is_set().then(|| uart.io().read().data().bits());
//         __dsb();
//         if let Some(byte) = byte {
//             if byte == recv {
//                 if last_byte.elapsed(systmr) <= byte_timeout {
//                     seq_no += 1;
//                 } else {
//                     seq_no = 1;
//                 }
//             } else {
//                 seq_no = 0;
//             }
//             *last_byte = timing::Instant::now(systmr);
//         }
//         if seq_no == n {
//             break 'skreeonk true
//         }
//     }
// }
