use bcm2835_lpa::{CM_PWM, GPIO, PWM0, SYSTMR};
use crate::arch::barrier::{data_synchronization_barrier};
use crate::boot::fmt::Uart1;
use crate::uprintln;
use core::fmt::Write;

// fn spi0_init(
//     spi_device: &SPI0,
//     gpio: &GPIO
// ) {
//     data_synchronization_barrier();
//
//     gpio.gpfsel1().modify(|_, w| {
//         w.fsel10().spi0_mosi()
//             .fsel11().spi0_sclk()
//     });
//     gpio.gpfsel0().modify(|_, w| {
//         w.fsel7().spi0_ce1_n()
//             .fsel8().spi0_ce0_n()
//             .fsel9().spi0_miso()
//     });
//
//     data_synchronization_barrier();
//
//     spi_device.cs().modify(|_, w| {
//         w
//             .len().clear_bit()
//             // set MIMO to write mode
//             .ren().clear_bit()
//             .intr().clear_bit()
//             .intd().clear_bit()
//             .dmaen().clear_bit()
//             .ta().clear_bit()
//             .cspol().clear_bit()
//             .clear().both()
//             .cpol().clear_bit() // doesn't matter
//             .cpha().set_bit() // doesn't matter
//             .cs().variant(0) // doesn't matter
//     });
//
//     data_synchronization_barrier();
// }
//
// fn spi0_set_clock_divider(spi: &SPI0, cd: u16) {
//     data_synchronization_barrier();
//
//     if !cd.is_power_of_two() {
//         return
//     }
//     unsafe {
//         spi.clk()
//             .write_with_zero(|w| w.cdiv().variant(cd))
//     }
//
//     data_synchronization_barrier();
// }
//
// fn spi0_write_bytes(uart: &mut Uart1, spi: &SPI0, bytes: &[u8]) {
//     uprintln!(uart, "write_bytes");
//
//     data_synchronization_barrier();
//
//     // TA=1, REN=0, CLR=11
//     // begin transaction, MIMO in write mode
//     spi.cs().modify(|_, w| {
//         w.ta().set_bit().ren().clear_bit().clear().both()
//     });
//
//     // dump bytes
//     for byte in bytes.iter() {
//         // for j in 0..8 {
//             // let x = (byte & (1 << j)) != 0;
//             // let b : u8 = if x { 0xff } else { 0x00 };
//             // data_synchronization_barrier();
//             // uprintln!(uart, "write byte {}", *byte);
//             // data_synchronization_barrier();
//             // for i in 0..100 {
//                 loop {
//                     // clear RX FIFO
//                     let cs = spi.cs().read();
//                     if cs.rxd().bit_is_set() {
//                         spi.cs().modify(|_, w| w.clear().rx());
//                     }
//                     // check whether we can add to the TX FIFO
//                     if cs.txd().bit_is_set() {
//                         break
//                     }
//                     // uprintln!(uart, "{:?}", cs);
//                 }
//                 // write one byte to the FIFO
//                 spi.fifo().write(|w| {
//                     w.data().variant(*byte as u32)
//                 });
//             // }
//         // }
//     }
//
//     // uprintln!(uart, "done write");
//
//     loop {
//         _marker();
//         spi.cs().modify(|_, w| w.clear().rx());
//         if spi.cs().read().done().bit_is_set() {
//             break
//         }
//         // uprintln!(uart, "status: {:?}", spi.cs().read());
//         data_synchronization_barrier();
//     }
//
//     spi.cs().modify(|_, w| w.ta().clear_bit());
//
//     data_synchronization_barrier();
// }
//
// #[no_mangle]
// #[inline(never)]
// pub extern "C" fn _marker() {
//     unsafe { asm!("") }
// }
//
// pub fn lab4(uart: &mut Uart1, spi: &SPI0, gpio: &GPIO) {
//     spi0_init(spi, gpio);
//     uprintln!(uart, "SPI0 init done");
//     spi0_set_clock_divider(spi, 64);
//     // spi0_set_clock_divider(spi, 1<<15);
//     // 75=300ns
//     // 64=256ns
//     uprintln!(uart, "SPI0 clock divider set to {}", {
//         data_synchronization_barrier();
//         let x = spi.clk().read().cdiv().bits();
//         data_synchronization_barrier();
//         x
//     });
//     // ≥50us ; use 60us so 200 bytes
//     // 30 leds *  3 = 90
//
//     const NLED : usize = 30;
//     const NBYT : usize = NLED * 3;
//
//     let mut buf : [u8; NBYT] = [0; NBYT];
//
//     for i in 0..90 {
//         buf[i] = 0xff;
//     }
//
//     // 1 bit = 256ns
//     // so 1000 bits is 256us
//     // 1000 bits is
//     const PBYT : usize = NBYT * 4;
//     const PLOW : usize = 1000;
//     const NBYT2 : usize = PBYT + PLOW;
//
//     let mut buf2 : [u8; NBYT2] = [0; NBYT2];
//     // buf transfer
//     {
//         for i in 0..buf.len() {
//             let mut c = [0u8; 4];
//             let b = buf[i];
//             for j in 0..8 {
//                 let bit = (b & (1 << j)) != 0;
//                 // 0 = 1H 3L
//                 // 1 = 3H 1L
//                 let nybble = if bit {
//                     0b1110
//                 } else {
//                     0b1000
//                 };
//                 c[j / 2] |= nybble << (4 * (j % 2));
//             }
//             buf2[i*4..i*4+4].copy_from_slice(&c);
//         }
//         for j in buf.len()*4..buf2.len() {
//             buf2[j] = 0;
//         }
//     }
//
//     uprintln!(uart, "BUFFER READY");
//
//     spi0_write_bytes(uart, spi, &buf2);
//
//     uprintln!(uart, "DONE");
// }

fn st_read(st: &SYSTMR) -> u64 {
    // CHI|CLO runs on a 1MHz oscillator
    data_synchronization_barrier();
    let hi32 = {st.chi().read().bits() as u64 } << 32;
    let t = hi32 | { st.clo().read().bits() as u64 };
    data_synchronization_barrier();
    t
}

fn delay_micros(st: &SYSTMR, micros: u64) {
    let begin = st_read(st);
    while st_read(st) < (begin + micros) {}
}

pub fn lab4(uart: &mut Uart1, st: &SYSTMR, pwm: &PWM0, cm_pwm: &CM_PWM, gpio: &GPIO) {
    data_synchronization_barrier();

    // gpio18 ALT5 = bpin 12 ; PWM0 <-> channel 1?
    gpio.gpfsel1().modify(|_, w| w.fsel12().pwm0_0());

    data_synchronization_barrier();

    cm_pwm.cs().write(|w| {
        // unsafe {
        //      w.bits(0x5a00_0000 | 0x0000_0006)
        // }
        w.passwd().passwd()
            .src().pllc()
            // .src().xosc()
    });
    delay_micros(st, 110000);
    while cm_pwm.cs().read().busy().bit_is_set() {
        delay_micros(st, 10);
    }
    cm_pwm.div().write(|w| {
        unsafe {
            // divider=25 -> 50ns
            // w.bits(0x5a00_0000 | (25 << 12))
            // divider=150 -> 300ns
            w.bits(0x5a00_0000 | (150 << 12))
            // w.bits(0x5a00_0000 | (4095 << 12))
        }
    });
    cm_pwm.cs().write(|w| {
        w.passwd().passwd()
            .src().pllc()
            // .src().xosc()
            .enab().set_bit()
        // unsafe {
        //     w.bits(0x5a00_0000 | 0x0000_0006 | 0x0000_0010)
        // }
    });

    uprintln!(uart, "CM_PWM.CS={:?}", cm_pwm.cs().read());
    uprintln!(uart, "CM_PWM.DIV={:?}", cm_pwm.div().read());

    data_synchronization_barrier();

    // pwm_clk should be running on a 50ns cycle now
    // then we want M/S mode ; range=24
    // 0: 7 clocks high, 18 clocks low
    // 1: 18 clocks high, 7 clocks low
    // 18+7=25

    // on a 300ns cycle
    // 0: 1 clock high, 3 clocks low
    // 1: 3 clocks high, 1 clock low

    let t1h = 3; // 7
    let t0h = 1; // 18

    pwm.rng1().write(|w| {
        // unsafe { w.bits(25) }
        unsafe { w.bits(4) }
    });
    pwm.ctl().write(|w| {
        w
            // MSEN=1 USEF=1 POLA=0 SBIT=0 SPTL=0 MODE=PWM PWEN=1 CLRF=1
            .msen1().set_bit()
            .clrf1().set_bit()
            .usef1().set_bit()
            .pola1().clear_bit()
            .sbit1().clear_bit()
            .rptl1().clear_bit()
            // .sbit1().set_bit()
            // .rptl1().set_bit()
            .mode1().pwm()
            .pwen1().set_bit()
            .pwen2().clear_bit()
    });
    pwm.sta().write(|w| w.sta1().set_bit());

    uprintln!(uart, "PWM.RNG1={:?}", pwm.rng1().read());
    uprintln!(uart, "PWM.CTL={:?}", pwm.ctl().read());
    uprintln!(uart, "PWM.STA={:?}", pwm.sta().read());

    let mut buf : [u8; 90] = [0xff; 90];
    let mut i = 0;
    loop {
        for j in 0..30 {
            // let rgb = hsv_to_rgb((i % 360) as f32, 1., 1.);
            // uprintln!(uart, "RGB={} {} {}", rgb.0, rgb.1, rgb.2);
            // let rgb = (
            //     (i % 255) as u8,
            //     15,
            //     127
            //     );
            // buf[3*j..3*(j+1)].copy_from_slice(&[rgb.1, rgb.0, rgb.2]);
            // GRB order
            buf[3*j..3*(j+1)].copy_from_slice(if j == (i % 30) {
                &[0,255,0]
            } else if j == (i+1) % 30 {
                &[255,0,0]
            } else if j == (i+2) % 30 {
                &[0,0,255]
            } else {
                &[0,0,0]
            });
        }
        // {
        // }
        //
        i += 1;

        data_synchronization_barrier();
        for &byte in &buf {
            for j in 0..8 {
                let is_high = (byte & (1 << j)) != 0;
                let m = if is_high { t1h } else { t0h };
                while pwm.sta().read().full1().bit_is_set() {}
                pwm.fif1().write(|w| {
                    unsafe { w.bits(m) }
                });
            }
        }
        data_synchronization_barrier();


        delay_micros(st, 70);

        pwm.ctl().write(|w| {
            w.pwen1().clear_bit()
        });

        delay_micros(st, 100000);

        pwm.ctl().write(|w| {
            w
                // MSEN=1 USEF=1 POLA=0 SBIT=0 SPTL=0 MODE=PWM PWEN=1 CLRF=1
                .msen1().set_bit()
                .clrf1().set_bit()
                .usef1().set_bit()
                .pola1().clear_bit()
                .sbit1().clear_bit()
                .rptl1().clear_bit()
                // .sbit1().set_bit()
                // .rptl1().set_bit()
                .mode1().pwm()
                .pwen1().set_bit()
                .pwen2().clear_bit()
        });
    }

    //
    // uprintln!(uart, "holding low for 70us");
    //
    // uprintln!(uart, "PWM.RNG1={:?}", pwm.rng1().read());
    // uprintln!(uart, "PWM.CTL={:?}", pwm.ctl().read());
    // uprintln!(uart, "PWM.STA={:?}", pwm.sta().read());
    //
    // delay_micros(st, 70);
    //
    // uprintln!(uart, "done");
    //
    // uprintln!(uart, "PWM.RNG1={:?}", pwm.rng1().read());
    // uprintln!(uart, "PWM.CTL={:?}", pwm.ctl().read());
    // uprintln!(uart, "PWM.STA={:?}", pwm.sta().read());
    //
    // pwm.ctl().write(|w| {
    //     w.pwen1().clear_bit()
    // });
    //
}

// fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8,u8,u8) {
//     let c = v * s;
//     let h2 = h/60.;
//     let h3 = (h2 as i8 % 2) - 1;
//     let x = c * (1. - h3.abs() as f32);
//
//     fn hhh(c: f32, x: f32, h: f32) -> (f32, f32, f32) {
//         if 0. <= h && h < 1. { (c,x,0.) }
//         else if 1. <= h && h < 2. { (x,c,0.) }
//         else if 2. <= h && h < 3. { (0.,c,x) }
//         else if 3. <= h && h < 4. { (0.,x,c) }
//         else if 4. <= h && h < 5. { (x,0.,c) }
//         else if 5. <= h && h < 6. { (c,0.,x) }
//         else { (0.,0.,0.) }
//     }
//
//     let (r1,g1,b1) = hhh(c,x,h2);
//     let m = v-c;
//     let (r,g,b) = (r1+m,g1+m,b1+m);
//     ((r * 255.) as u8, (g * 255.) as u8, (b * 255.) as u8)
// }

// fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
//     let h = h/360.;
//     let i = (h * 6.) as u8 as f32;
//     let f = h * 6. - i;
//     let p = v * (1. - s);
//     let q = v * (1. - f * s);
//     let t = v * (1. - (1. - f) * s);
//
//     let (r,g,b) = match (i as i32) % 6 {
//         0 => (v,t,p),
//         1 => (q,v,p),
//         2 => (p,v,t),
//         3 => (p,q,v),
//         4 => (t,p,v),
//         5 => (v,p,q),
//         _ => (1.,1.,1.),
//     // case 0: r = v, g = t, b = p; break;
//     // case 1: r = q, g = v, b = p; break;
//     // case 2: r = p, g = v, b = t; break;
//     // case 3: r = p, g = q, b = v; break;
//     // case 4: r = t, g = p, b = v; break;
//     // case 5: r = v, g = p, b = q; break;
//     };
//
//     (
//     (r * 255.) as u8,
//     (g * 255.) as u8,
//     (b * 255.) as u8,
//     )
// }
