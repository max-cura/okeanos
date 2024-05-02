// TSOP (in) pin is 13
// TSAL (out) pin is 12

use core::time::Duration;
use bcm2835_lpa::{CM_PWM, GPIO, PWM0, SYSTMR};
use crate::arm1176::__dsb;
use crate::{send_blocking, sendln_blocking};
use crate::timing::{delay_micros, Instant};

// 40/80 = 40 optical cycles
const BURST_PWM_DUTY: u32 = 40;
const BURST_PWM_RANGE: u32 = 80;

const BURST_CYCLES : usize = 20;

// receiver gets cycle start at 7 to 15 cycle delay (~150us~350us)
// receiver goes active low for t_p time, where t_q-5cy < t_p < t_q+6cy where t_q is the actual
// burst length (-100us < Dt_p < +130us)

// 13.16us
const CYCLE_DURATION_NANOS: u64 = 26320;
#[allow(dead_code)]
const CYCLE_DURATION: Duration = Duration::from_nanos(CYCLE_DURATION_NANOS);

const CYCLE_HIGH_DURATION: Duration = Duration::from_nanos(13160);
const CYCLE_LOW_DURATION: Duration = Duration::from_nanos(13160);

// 20 optical cycles @ 38kHz ~= 500us
// so, transmitting all 1's, we get 5ms/bit so 200 bits/second, 200 bursts/second
const PDC_ONE_PULSE_COUNT: usize = 1;
const PDC_ZERO_PULSE_COUNT: usize = 2;
const PDC_ONE_PULSE_DURATION: Duration = Duration::from_nanos(CYCLE_DURATION_NANOS * 20);
const PDC_ZERO_PULSE_DURATION: Duration = Duration::from_nanos(CYCLE_DURATION_NANOS * 40);
const PDC_ONE_SILENCE: Duration = Duration::from_nanos(CYCLE_DURATION_NANOS * 80);
const PDC_ZERO_SILENCE: Duration = Duration::from_nanos(CYCLE_DURATION_NANOS * 60);
const PDC_BIT_DURATION_NANOS: u64 = CYCLE_DURATION_NANOS * 100;

pub const PDC_BIT_DURATION: Duration = Duration::from_nanos(PDC_BIT_DURATION_NANOS);

// use pulse-distance coding
//  1 BIT = BURST of 20  then quiet for 80 cycles
//  0 BIT = BURST of 40 then quiet for 60 cycles

const XMIT_FIFO_LEN : usize = 8;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum TxPhase {
    Idle, PulseHigh, PulseLow, Distance
}

pub struct IrTransmitter {
    pub byte: Option<u8>,
    // fifo: [u8; XMIT_FIFO_LEN],
    // fifo_front: usize,
    // fifo_back: usize,
    // fifo_len: usize,
    bit_no: u8,
    last_burst_began: Instant,
    // disabled_pwm: bool,
    // before_next_burst: Duration,
    // tx_on: bool,
    // high_for: Duration,
    cycle_no: usize,
    cycle_count: usize,
    last_peak: Instant,

    phase: TxPhase,

    deb: usize,
}

impl IrTransmitter {
    pub fn new(st: &SYSTMR) -> Self {
        Self {
            byte: None,
            // fifo: [0; XMIT_FIFO_LEN],
            // fifo_front: 0,
            // fifo_back: 0,
            // fifo_len: 0,
            bit_no: 7,
            last_burst_began: Instant::now(st),
            // disabled_pwm: false,
            // before_next_burst: Duration::from_micros(0),
            // tx_on: false,
            // high_for: Duration::from_micros(0),
            cycle_no: 0,
            cycle_count: 0,
            last_peak: Instant::now(st),
            phase: TxPhase::Idle,

            deb: 0,
        }
    }

    // pub fn can_push(&self) -> bool {
    //     self.byte.is_none()
    //     // self.fifo_len < XMIT_FIFO_LEN
    // }
    //
    // pub fn try_push(
    //     &mut self,
    //     byte: u8
    // ) -> bool {
    //     if self.byte
    //     // if self.fifo_len < XMIT_FIFO_LEN {
    //     //     self.fifo_len += 1;
    //     //     self.fifo[self.fifo_front] = byte;
    //     //     self.fifo_front += 1;
    //     //     if self.fifo_front == XMIT_FIFO_LEN {
    //     //         self.fifo_front = 0;
    //     //     }
    //     //     true
    //     // } else {
    //     //     false
    //     // }
    // }

    pub fn idle(&self) -> bool { self.phase == TxPhase::Idle }

    pub fn tick(
        &mut self,
        // pwm: &PWM0,
        gpio: &GPIO,
        st: &SYSTMR,
    ) {
        fn ir_low(gpio: &GPIO) {
            // send_blocking!(".");
            __dsb();
            unsafe { gpio.gpclr0().write_with_zero(|w| w.clr12().clear_bit_by_one()) };
            __dsb();
        }
        fn ir_high(gpio: &GPIO) {
            // send_blocking!("|");
            __dsb();
            unsafe { gpio.gpset0().write_with_zero(|w| w.set12().set_bit()) };
            __dsb();
        }

        self.deb += 1;

        // nothing to do
        // if self.fifo_len == 0 && self.phase == TxPhase::Idle {
        //     return false
        // }
        if self.byte.is_none() && self.phase == TxPhase::Idle {
            return
        }

        let now = Instant::now(st);

        let peak_elapsed = self.last_peak.elapsed_to(now);

        if self.phase == TxPhase::PulseHigh {
            if peak_elapsed >= CYCLE_HIGH_DURATION {
                sendln_blocking!("PEH={peak_elapsed:?}@{}", self.deb);
                self.deb = 0;

                ir_low(gpio);
                // for now, Instant::now() b/c of the sendln
                self.last_peak = Instant::now(st);
                self.phase = TxPhase::PulseLow;
            }
            return
        } else if self.phase == TxPhase::PulseLow {
            if peak_elapsed >= CYCLE_LOW_DURATION {
                sendln_blocking!("PEL={peak_elapsed:?}@{}", self.deb);
                self.deb = 0;

                self.cycle_no += 1;
                if self.cycle_no == self.cycle_count {
                    self.phase = TxPhase::Distance;
                } else {
                    ir_high(gpio);
                    // for now, Instant::now() b/c of the sendln
                    self.last_peak = Instant::now(st);
                    // send_blocking!("^");
                    self.phase = TxPhase::PulseHigh;
                }
            }
            return
        } else if self.phase == TxPhase::Distance {
            let lbb_elapsed = self.last_burst_began.elapsed_to(now);
            if lbb_elapsed >= PDC_BIT_DURATION {
                self.phase = TxPhase::Idle;
                if self.byte.is_none() {
                    return
                } else {
                    // DONT return - can get another bit now
                }
            } else {
                return
            }
        }

        // phase is Idle and byte is Some
        let byte = self.byte.unwrap();

        // let byte = self.fifo[self.fifo_back];
        let bit = (byte & (0x80 >> self.bit_no)) != 0;
        // let bit = ((byte >> self.bit_no) & 1) == 1;
        self.bit_no -= 1;
        if self.bit_no == 0 {
            // send_blocking!("\n");

            // self.fifo_len -= 1;
            // self.fifo_back += 1;
            // if self.fifo_back == XMIT_FIFO_LEN {
            //     self.fifo_back = 0;
            // }
            self.bit_no = 7;
            self.byte = None;
        }

        self.cycle_count = if bit {
        //     // send_blocking!("#{lbb_elapsed:?}");
            BURST_CYCLES * PDC_ONE_PULSE_COUNT
        } else {
        //     // send_blocking!("-{lbb_elapsed:?}");
            BURST_CYCLES * PDC_ZERO_PULSE_COUNT
        };
        self.cycle_no = 0;
        self.last_burst_began = now;
        self.last_peak = now;
        self.phase = TxPhase::PulseHigh;
        self.deb = 0;
        ir_high(gpio);
    }
}

pub struct IrReceiver {
    received_bits: u8,
    received_bit_count: u8,
    last_burst_began: Instant,
    // whether the IrReceiver is waiting on a falling edge (false) or a rising edge (true)
    edge_link: bool,
    in_bit: bool,
}

// falling edge delay: 7 to 15 cycles
// burst low jitter: ±6 cycles
// one pulse: 20cy ±6cy -> 14..26 ; use 12-28
// zero pule: 40cy ±6cy -> 34..46 ; use 32-38

#[derive(Debug, thiserror::Error, Copy, Clone)]
pub enum IrRecvError {
    #[error("pulse gap was only {0:?}")]
    InsufficientPulseGap(Duration),
    #[error("unrecognized pulse length: {0:?}")]
    PulseLength(Duration),
    #[error("pulse gap too long")]
    PulseGapTooLong,
    #[error("unexpected rising edge")]
    UnexpectedRisingEdge,
    #[error("unexpected falling edge")]
    UnexpectedFallingEdge,
}

const RX_LBB_JITTER_NANOS : u64 = CYCLE_DURATION_NANOS * 10;
// jitter in bit gap: 15cy-7cy=8cy since jitter is from falling edge delay, add 25%
#[allow(dead_code)]
const RX_LBB_JITTER : Duration = Duration::from_nanos(RX_LBB_JITTER_NANOS);
// jitter in pulse length: ±6cy, add 33%
const RX_PULSE_JITTER : Duration = Duration::from_nanos(CYCLE_DURATION_NANOS * 8);

const MIN_BIT_GAP: Duration = Duration::from_nanos(PDC_BIT_DURATION_NANOS - RX_LBB_JITTER_NANOS);
const MAX_BIT_GAP: Duration = Duration::from_nanos(PDC_BIT_DURATION_NANOS + 3 * RX_LBB_JITTER_NANOS);

macro_rules! dump_timing {
    [$($i:ident),*] => {
        let pairs: &[(&str, Duration)] = &[
            $((stringify!($i), $i)),*
        ];
        for (s,d) in pairs {
            sendln_blocking!("{s}={d:?}");
        }
    };
}

impl IrReceiver {
    pub fn new(st: &SYSTMR) -> Self {
        dump_timing![
            CYCLE_DURATION,
            CYCLE_LOW_DURATION,
            CYCLE_HIGH_DURATION,
            PDC_BIT_DURATION,
            PDC_ZERO_PULSE_DURATION,
            PDC_ZERO_SILENCE,
            PDC_ONE_PULSE_DURATION,
            PDC_ONE_SILENCE,
            RX_LBB_JITTER,
            RX_PULSE_JITTER,
            MIN_BIT_GAP,
            MAX_BIT_GAP
        ];

        Self {
            received_bits: 0,
            received_bit_count: 0,
            last_burst_began: Instant::now(st),
            // waiting for falling edge
            edge_link: false,
            in_bit: false,
        }
    }

    fn _reset(&mut self) {
        // wait for next rising edge
        self.edge_link = false;
        self.in_bit = false;
        // discard data
        self.received_bits = 0;
        self.received_bit_count = 0;
        // don't do anything to LBB - we'll leave it as is
        // IrReceiver::tick()'s caller will take care of error timeouts
    }

    pub fn tick<'b>(
        &mut self,
        gpio: &GPIO,
        st: &SYSTMR,
    ) -> Result<Option<u8>, IrRecvError> {
        let elapsed = self.last_burst_began.elapsed(st);

        let res : Result<Option<u8>, IrRecvError> = try {
            __dsb();

            if gpio.gpeds0().read().eds13().bit_is_set() {
                unsafe { gpio.gpeds0().write_with_zero(|w| w.eds13().clear_bit_by_one()) }

                let level = gpio.gplev0().read().lev13().bit_is_set();
                __dsb();

                if !level {
                    if self.edge_link {
                        // we were in LOW and hit a falling edge
                        Err(IrRecvError::UnexpectedFallingEdge)?
                    }

                    self.last_burst_began = Instant::now(st);

                    // Falling edge
                    if elapsed < MIN_BIT_GAP {
                        Err(IrRecvError::InsufficientPulseGap(elapsed))?
                    } else {
                        self.edge_link = true; // important!
                        self.in_bit = true; // also important!

                        None
                    }
                } else {
                    if !self.edge_link {
                        Err(IrRecvError::UnexpectedRisingEdge)?
                    }
                    // Rising edge
                    let maybe_bit = if elapsed < PDC_ONE_PULSE_DURATION + RX_PULSE_JITTER {
                        if elapsed > PDC_ONE_PULSE_DURATION - RX_PULSE_JITTER {
                            // 1 bit
                            Some(1)
                        } else {
                            None
                        }
                    } else if elapsed > PDC_ZERO_PULSE_DURATION - RX_PULSE_JITTER {
                        if elapsed < PDC_ZERO_PULSE_DURATION + RX_PULSE_JITTER {
                            Some(0)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let bit = maybe_bit.ok_or(IrRecvError::PulseLength(elapsed))?;

                    // if bit == 1 {
                    //     send_blocking!("\x1b[31m#\x1b[0m");
                    // //     sendln_blocking!("#")
                    // } else {
                    //     send_blocking!("\x1b[32m%\x1b[0m");
                    // //     sendln_blocking!("-")
                    // }

                    self.edge_link = false; // important!
                    self.in_bit = false;

                    // LSb first
                    self.received_bits |= bit << self.received_bit_count;
                    self.received_bit_count += 1;
                    if self.received_bit_count == 8 {
                        self.received_bit_count = 0;
                        let byte = self.received_bits;

                        self.received_bits = 0;
                        Some(byte)
                    } else {
                        None
                    }
                }
            } else {
                // no more GPIO so just DSB
                __dsb();

                if elapsed > MAX_BIT_GAP && self.in_bit {
                    Err(IrRecvError::PulseGapTooLong)?
                }
                None
            }
        };
        if res.is_err() {
            self._reset();
        }
        res
    }
}

pub fn init(
    gpio: &GPIO,
    pwm: &PWM0,
    cm_pwm: &CM_PWM,
    st: &SYSTMR,
) {
    __dsb();

    gpio.gpfsel1().modify(|_, w| {
        w
            .fsel12().output()
            // .fsel12().pwm0_0()
            .fsel13().input()
    });

    // detect only FEN and REN
    gpio.gpafen0().modify(|_, w| w.afen13().clear_bit());
    gpio.gparen0().modify(|_, w| w.aren13().clear_bit());
    gpio.gplen0().modify(|_, w| w.len13().clear_bit());
    gpio.gphen0().modify(|_, w| w.hen13().clear_bit());
    gpio.gpren0().modify(|_, w| w.ren13().set_bit());
    gpio.gpfen0().modify(|_, w| w.fen13().set_bit());
    unsafe { gpio.gpeds0().write_with_zero(|w| w.eds13().clear_bit_by_one()); }

    __dsb();

    // cm_pwm.cs().write(|w| {
    //     w.passwd().passwd()
    //         .src().pllc()
    // });
    //
    // // SAFETY: pre- and post-DSB from delay_micros
    // delay_micros(st, 110000);
    //
    // while cm_pwm.cs().read().busy().bit_is_set() {
    //     // SAFETY: pre- and post-DSB from delay_micros
    //     delay_micros(st, 10);
    // }
    // cm_pwm.div().write(|w| {
    //     // unsafe {
    //     //     // 6579 = 13.16us?
    //     //     w.bits(0x5a00_0000 | (6579 << 12))
    //     // }
    //     unsafe {
    //         // 13.16us = 252 + 716.8/1024
    //         // divi=252, divf=717
    //         // close enough
    //         w
    //             .passwd().passwd()
    //             .divi().bits(252)
    //             .divf().bits(717)
    //     }
    // });
    // cm_pwm.cs().write(|w| {
    //     w.passwd().passwd()
    //         // .src().pllc() // 500 MHz
    //         .src().xosc() // 19.2 MHz
    //         .enab().set_bit()
    // });
    //
    // __dsb();
    //
    // // sendln_blocking!(
    // //     "CM_PWM.CS={:?}",
    // //     cm_pwm.cs().read()
    // // );
    // // sendln_blocking!(
    // //     "CM_PWM.DIV={:?}",
    // //     cm_pwm.div().read()
    // // );
    // //
    // // __dsb();
    //
    // pwm.rng1().write(|w| {
    //     unsafe { w.bits(BURST_PWM_RANGE) }
    // });
    // pwm.ctl().modify(|_, w| {
    //     w
    //         .msen1().clear_bit()
    //         // clear fifos
    //         .clrf1().set_bit()
    //         // use fifos
    //         .usef1().set_bit()
    //         // normal polarity 1=HI 0=LO
    //         .pola1().clear_bit()
    //         // write 0 when no transmission
    //         .sbit1().clear_bit()
    //         .rptl1().clear_bit()
    //         // use PWM instead of serializer
    //         .mode1().pwm()
    //         // enable only PWM Channel 1
    //         // .pwen1().clear_bit()
    //         .pwen1().clear_bit()
    //         .pwen2().clear_bit()
    // });
    // pwm.sta().modify(|_, w| w.sta1().set_bit().berr().clear_bit());
    //
    // // don't do this: causes a bus error?
    // // pwm.ctl().modify(|_, w| {
    // //     w.pwen1().set_bit()
    // // });
    //
    // __dsb();
}

fn burst_pwm_enable(
    pwm: &PWM0
) {
    __dsb();

    // sendln_blocking!("E{:?}", pwm.ctl().read());

    pwm.ctl().modify(|_, w| {
        w.pwen1().set_bit()
    });

    __dsb();
}

fn burst_pwm_disable(
    pwm: &PWM0
) {
    __dsb();

    // sendln_blocking!("D{:?}", pwm.ctl().read());

    pwm.ctl().modify(|_, w| {
        w.pwen1().clear_bit()
    });

    __dsb();
}

fn burst_queue_flush(
    pwm: &PWM0
) {
    __dsb();

    while pwm.sta().read().empt1().bit_is_clear() {}

    __dsb();
}

fn burst_queue_push(
    pwm: &PWM0
) {
    __dsb();

    // sendln_blocking!("PWM_STA={:?} PWM_CTL={:?}", pwm.sta().read(), pwm.ctl().read());

    while pwm.sta().read().full1().bit_is_set() {}

    pwm.fif1().write(|w| {
        unsafe { w.bits(BURST_PWM_DUTY) }
    });

    __dsb();
}

fn burst_queue_silence(
    pwm: &PWM0,
) {
    __dsb();

    // sendln_blocking!("PWM_STA={:?} PWM_CTL={:?}", pwm.sta().read(), pwm.ctl().read());

    while pwm.sta().read().full1().bit_is_set() {}

    pwm.fif1().write(|w| {
        unsafe { w.bits(0) }
    });

    __dsb();
}

fn poll_input_nonblocking(
    gpio: &GPIO
) -> bool {
    __dsb();

    let bit = gpio.gplev0().read().lev21().bit_is_set();

    __dsb();

    bit
}
