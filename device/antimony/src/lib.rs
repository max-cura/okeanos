#![allow(incomplete_features)]
#![feature(naked_functions)]
#![feature(decl_macro)]
#![feature(adt_const_params)]
#![feature(trait_alias)]
#![feature(const_trait_impl)]
#![feature(never_type)]
#![feature(macro_metavar_expr)]
#![feature(generic_const_exprs)]
#![feature(const_format_args)]
#![feature(sync_unsafe_cell)]
#![feature(likely_unlikely)]
#![feature(pointer_is_aligned_to)]
#![feature(array_try_map)]
#![feature(iterator_try_collect)]
#![feature(ptr_as_ref_unchecked)]
#![feature(iter_intersperse)]
#![feature(ip_from)]
#![no_std]
extern crate alloc;

pub mod arch;
pub mod critical_section;
pub mod net;
pub mod peripherals;

use crate::arch::exception::Target;
use crate::arch::time::{Instant, delay, never, now};
use crate::arch::{Mie, Mstatus};
use crate::net::phy::_trap_mei;
use crate::peripherals::pin;
use crate::peripherals::uart::UartDevice;
use crate::peripherals::uart::{Mode, Uart, UartDev};
use core::arch::{asm, naked_asm};
use core::cell::{OnceCell, RefCell};
use core::fmt::Write;
use core::panic::PanicInfo;
use d1_pac::{Peripherals, UART0, UART2};
use log::{Level, LevelFilter, Metadata, Record};

core::arch::global_asm!(
    r#"
    .attribute arch, "rv64gc"
    "#
);

core::arch::global_asm!(
    r#"
.section ".head.start"
.globl _start
_start:
.option push
.option norelax
    la gp, __global_pointer$
.option pop
    j {KERNEL_START}
"#,
    KERNEL_START = sym __kernel_start
);

#[repr(C)]
pub struct EgonBt0Head {
    magic: [u8; 8],
    checksum: u32,
    length: u32,
    _padding: [u32; 3],
}
const _: () = const { assert!({ size_of::<EgonBt0Head>() + 4 } == 0x20) };
const EGON_BT0_STAMP_CHECKSUM: u32 = 0x5F0A6C39;
#[used]
#[unsafe(link_section = ".head.egon")]
static EGON_HEAD: EgonBt0Head = EgonBt0Head {
    magic: *b"eGON.BT0",
    checksum: EGON_BT0_STAMP_CHECKSUM,
    length: 0,
    _padding: [0; 3],
};

#[naked]
#[unsafe(no_mangle)]
pub extern "C" fn __kernel_start() -> ! {
    unsafe extern "C" {
        static amn_bss_start: [u64; 0];
        static amn_bss_end: [u64; 0];
        static amn_stack_end: [u64; 0];
    }
    unsafe {
        naked_asm!(
            r#"
            /* Clear cache & processor state */
                csrw mie, zero
                /* 21: MAEE (extend MMU address attribute)
                   22: THEADISAEE (enable T-HEAD C906 extended ISA) */
                li t1, 1 << 22 | 1 << 21
                /* 0x7c0: MXSTATUS *.
                csrs 0x7c0, t1
                /* 0-1: CACHE_SEL=both
                   4  : invalidate cache
                   5  : (CLR=0) ; dirty cache entries are not written out
                   16 : BHT (branch history table) invalidate
                   17 : BTB (branch target buffer) invalidate
                    */
                li t2, 0x30013
                /* 0x7c2: MCOR */
                csrw 0x7c2, t2

            /* clear BSS */
                la t0, {BSS_START}
                la t1, {BSS_END}
            1:
                bgeu t0, t1, 2f
                sd zero, 0(t0)
                addi t0, t0, 8
                j 1b
            2:

            /* initialize SP and FP */
                la sp, {STACK_INIT}
                andi sp, sp, -16 // align stack to 16 bytes
                add fp, sp, zero // initialize fp to point to stack

                la a0, {EGON_HEAD}
                j {KERNEL_MAIN}
            "#,
            BSS_START = sym amn_bss_start,
            BSS_END = sym amn_bss_end,
            STACK_INIT = sym amn_stack_end,
            EGON_HEAD = sym EGON_HEAD,
            KERNEL_MAIN = sym __kernel_main
        )
    }
}

#[global_allocator]
static HEAP: embedded_alloc::TlsfHeap = embedded_alloc::TlsfHeap::empty();

pub const APB_FREQ: u32 = 200_000_000;

pub extern "C" fn __kernel_main(_egon_head: *mut EgonBt0Head) -> ! {
    let peri = unsafe { Peripherals::steal() };

    // ---------------------------------------------------------------------------------------------
    // Initialize peripheral pins

    let pinmux = unsafe { peripherals::forge_pinmux() };
    assign_pins! {
        pinmux;
        let pb8 : PB8;
        let pb9 : PB9;
        let pb0 : PB0;
        let pb1 : PB1;
    }
    let _ = pinmux;

    delay(100);

    // ---------------------------------------------------------------------------------------------
    // Set up UART clock source; default is HOSC at 24MHz, but that doesn't work well if we want to
    // have a nice baud rate above 115200 for error reasons, so we want to use the PLL PERI(1X)
    // clock. There's no independent UART clock, so we use the APB1 clock instead, which is the
    // UART's direct upstream (it services very few other things).

    // APB1 only clocks for UART, TWI, SP1 (?)
    // bus clocks:
    //  - from lower to higher frequency, configure frequency division factor first, and then
    //    switch the clock source
    // HOSC is 24MHz, CLK32 - 32KHz, PSI_CLK - ??, PLL_PERI(1X) is 600MHz
    peri.CCU.apb1_clk().modify(|_, w| {
        w.factor_m()
            .variant(2) // divide: 12
            .factor_n()
            .n1() // divide: 1
    });
    peri.CCU.apb1_clk().modify(|_, w| {
        w.clk_src_sel().pll_peri_1x() // base: 600MHz
    });

    // Set up UART clock gating
    peri.CCU
        .uart_bgr()
        .modify(|_, w| w.uart0_gating().pass().uart2_gating().pass());
    peri.CCU
        .uart_bgr()
        .modify(|_, w| w.uart0_rst().deassert().uart2_rst().deassert());
    peri.GPIO
        .pb_pull0()
        .modify(|_, w| w.pc8_pull().pull_up().pc0_pull().pull_up());
    peri.GPIO.pd_cfg0().modify(|_, w| w.pd0_select().output());

    // ---------------------------------------------------------------------------------------------
    // Initialize UARTs with correct baud rates;

    critical_section::with(|cs| {
        let uart0: Uart<_, { Mode::Direct }> = Uart::new(
            UartDev::<UART0, { pin!(PB8 as UART0_TX) }, { pin!(PB9 as UART0_RX) }>::new(pb8, pb9),
            921600,
        )
        .unwrap();
        UART0
            .borrow_ref(cs)
            .set(uart0)
            .expect("UART0 already initialized");
    });
    let _uart2: Uart<_, { Mode::Direct }> = Uart::new(
        UartDev::<UART2, { pin!(PB0 as UART2_TX) }, { pin!(PB1 as UART2_RX) }>::new(pb0, pb1),
        115200,
    )
    .unwrap();
    dprintln!("\r\n\r\n[antimony] UARTs configured");

    // ---------------------------------------------------------------------------------------------
    // Initialize global heap.

    unsafe extern "C" {
        static amn_exec_end: [u8; 0];
    }
    const MEM_END: usize = 0x4000_0000 + 0x2000_0000;
    let prog_end_addr = (&raw const amn_exec_end).addr();
    let heap_size = MEM_END - prog_end_addr;
    dprintln!("heap_start={prog_end_addr:x} heap_end={MEM_END:x} size={heap_size:x}");
    unsafe { HEAP.init(prog_end_addr, heap_size) };
    dprintln!("[antimony] set up heap");

    // ---------------------------------------------------------------------------------------------
    // Initialize exception handling for network stack.

    #[repr(align(16))]
    struct Aligned16<T>(T);
    static mut VECTOR: Aligned16<[u32; 256]> = Aligned16([0; 256]);

    unsafe {
        arch::exception::VectorBuilder::new()
            .set_unchecked(11, Target::Jump((_trap_mei as *const ()).addr()))
            .install(&raw mut (VECTOR.0))
            .unwrap();

        // 1. configure UART interrupt vector number to request UART interrupt (20)
        // peri.PLIC.ctrl().write(|w| w.ctrl().m());
        peri.PLIC.mth().write(|w| w.priority().p10());
        peri.PLIC.prio(20).write(|w| w.bits(0x1f));
        peri.PLIC.prio(18).write(|w| w.bits(0x18));
        peri.PLIC.mie(0).write(|w| w.bits((1 << 20) | (1 << 18)));

        // 2. in interrupt mode, configure UART_IER to enable corresponding interrupt according
        //    to requirements
        peri.UART0
            .fcr()
            .write(|w| w.tft().two_characters().fifoe().set_bit());
        peri.UART0.ier().write(|w| w);

        peri.UART2.fcr().write(|w| {
            w.rt()
                .two_less_than_full()
                .tft()
                .two_characters()
                .fifoe()
                .set_bit()
        });
        peri.UART2.ier().write(|w| w.erbfi().enable());

        critical_section::with(|_| {
            arch::mstatus::set_bits(Mstatus(0).with_mie(true));
            arch::mie::set_bits(Mie(0).with_meie(true).with_msie(true));
        });
    }
    delay(100); // no particular reason
    dprintln!("[antimony] set up exception handling");

    // ---------------------------------------------------------------------------------------------
    // Miscellaneous hardware

    critical_section::with(|_| {
        dprint!("enabling caches...");
        arch::mem::enable_dcache();
        arch::mem::enable_icache();
        dprintln!("done");
    });
    let hartid: usize;
    unsafe {
        asm!("csrr {}, mhartid", out(reg) hartid);
    }
    dprintln!("hartid={:x} mapbadddr={:x}", hartid, unsafe {
        arch::mapbaddr::raw::read()
    });

    // ---------------------------------------------------------------------------------------------
    // Sanity checks

    // Sanity check: interrupt-based printing
    println!(
        "PRINT TEST 123456789 123456789 123456789 123456789 123456789 123456789 123456789 123456789\r\n\
        PRINT TEST 123456789 123456789 123456789 123456789 123456789 123456789 123456789 123456789\r\n\
        PRINT TEST 123456789 123456789 123456789 123456789 123456789 123456789 123456789 123456789\r\n\
        PRINT TEST OK"
    );

    // ---------------------------------------------------------------------------------------------
    // Initialize `log` crate
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(LOG_LEVEL.to_level_filter()))
        .expect("failed to set logger");

    // ---------------------------------------------------------------------------------------------
    // Run network stack

    unsafe { GENESIS = now() };

    net::run();

    // ---------------------------------------------------------------------------------------------
    // Should be unreachable, but this is where we go if the network stack exits.

    __kernel_restart()
}

static mut GENESIS: Instant = never();
pub fn smoltcp_now() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_micros(
        i64::try_from((now() - unsafe { GENESIS }).as_micros()).unwrap(),
    )
}

const LOG_LEVEL: Level = Level::Trace;

static LOGGER: SerialLogger = SerialLogger;

struct SerialLogger;
impl log::Log for SerialLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("{}: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

pub extern "C" fn __kernel_restart() -> ! {
    dprintln!("[antimony] shutdown (halt)");
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    if let Some(loc) = info.location() {
        dprintln!(
            "Panic occurred at file '{}' line {}:\n",
            loc.file(),
            loc.line()
        );
    } else {
        dprintln!("Panic occurred at unknown location.\n");
    }
    let msg = info.message();
    critical_section::with(|cs| {
        let mut lock = UART0.borrow_ref_mut(cs);
        let out = lock.get_mut().unwrap();
        let _ = core::fmt::write(out, format_args!("{}\r\n", msg));
    });

    __kernel_restart()
}

// please don't use this, thanks
static UART0: critical_section::Mutex<
    RefCell<
        OnceCell<
            Uart<
                UartDev<UART0, { pin!(PB8 as UART0_TX) }, { pin!(PB9 as UART0_RX) }>,
                { Mode::Direct },
            >,
        >,
    >,
> = critical_section::Mutex::new(RefCell::new(OnceCell::new()));
pub macro println($($arg:tt)*) {
    let mut s = ::alloc::format!($($arg)*);
    s.push_str("\r\n");
    $crate::net::phy::write_string(s);
}
pub macro print($s:literal$($arg:tt)*) {
    let s = ::alloc::format!($s, $($arg)*);
    $crate::net::phy::write_string(s);
}
pub macro dprintln($($arg:tt)*) {
    critical_section::with(|cs| {
        let mut lock = UART0.borrow_ref_mut(cs);
        let out = lock.get_mut().unwrap();
        let _ = ::core::write!(out, $($arg)*);
        let _ = out.write_str("\r\n");
    })
}
pub macro dprint($($arg:tt)*) {
    critical_section::with(|cs| {
        let mut lock = UART0.borrow_ref_mut(cs);
        let out = lock.get_mut().unwrap();
        let _ = ::core::write!(out, $($arg)*);
    })
}
