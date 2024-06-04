use crate::reactor::blinken::Blinken;
use crate::stub::{__symbol_relocation_stub__, __symbol_relocation_stub_end__};
use bcm2835_lpa::Peripherals;
use core::alloc::Layout;
use core::any::Any;
use core::fmt::Arguments;
use lab4_common::muart;
use lab4_common::reactor::io_theseus::IrDriver;
use lab4_common::reactor::log_uart1_raw::RawUart1Logger;
use lab4_common::reactor::protocol_theseus::BootProtocol;
use lab4_common::reactor::{Indicators, Io, IoTimeouts, Logger, ProtocolEnum, Reactor};
use lab4_common::relocation::Relocation;
use muart::__flush_tx;
use theseus_common::theseus::v1;

// pub mod handshake;
// mod v1;
pub mod blinken;

struct Blink2;

impl Indicators for Blink2 {
    fn io_did_write(&self, rz: &Reactor, wrote: bool) {
        Blinken._27(&rz.peri.GPIO, wrote)
    }

    fn io_is_receiving(&self, rz: &Reactor, receiving: bool) {
        Blinken._47(&rz.peri.GPIO, receiving)
    }

    fn io_input_overrun(&self, rz: &Reactor, overrun: bool) {
        todo!()
    }
}

pub fn run() {
    let peri = unsafe { Peripherals::steal() };
    Blinken::init(&peri.GPIO);
    // 115200
    muart::uart1_init(&peri.GPIO, &peri.AUX, &peri.UART1, 270);

    let env = lab4_common::reactor::Env {
        __unsafe_program_end__: unsafe {
            core::ptr::addr_of!(crate::stub::__symbol_exec_end__) as *mut u8
        },
        __unsafe_memory_end__: (512 * 1024 * 1024) as *mut u8,
    };
    let mut reactor = Reactor::new(peri, env).expect("failed to allocate buffers for Reactor");

    let mut protocol = BootProtocol::new(&mut reactor, final_relocation);
    let mut logger = RawUart1Logger;
    let mut io = IrDriver::new(
        &mut reactor,
        /* ibuf */ Layout::from_size_align(0x10000, 4).unwrap(),
        /* obuf */ Layout::from_size_align(0x10000, 4).unwrap(),
        /* timeouts */ BootProtocol::default_driver_timeouts(),
    )
    .expect("failed to allocate buffers for IR driver");

    let indicators = Blink2;

    reactor.run(
        &mut logger,
        &mut io,
        &indicators,
        ProtocolEnum::BootProtocol(protocol),
    );
}

unsafe fn final_relocation(
    rz: &mut Reactor,
    io: &mut dyn Io,
    // logger: &mut dyn Logger,
    relocation: Relocation,
) -> ! {
    let blinken = Blinken::init(&rz.peri.GPIO);
    blinken.set(&rz.peri.GPIO, 0);
    // blinken._5(&rz.peri.GPIO, false);

    let stub_dst = relocation.stub_entry;
    let kernel_dst = relocation.base_address_ptr;
    let kernel_src = relocation.side_buffer_ptr;
    let kernel_copy_len = relocation.relocate_first_n_bytes;
    let kernel_entry = relocation.base_address_ptr;

    let stub_begin = core::ptr::addr_of!(__symbol_relocation_stub__);
    let stub_end = core::ptr::addr_of!(__symbol_relocation_stub_end__);

    let stub_len = stub_end.byte_offset_from(stub_begin) as usize;

    crate::sendln_blocking!("[device:v1]: relocation_stub parameters:");
    crate::sendln_blocking!("\tstub destination={stub_dst:#?}");
    crate::sendln_blocking!("\tstub code={stub_begin:#?}");
    crate::sendln_blocking!("\tstub length={stub_len:#?}");
    crate::sendln_blocking!("\tcopy to={kernel_dst:#?}");
    crate::sendln_blocking!("\tcopy from={kernel_src:#?}");
    crate::sendln_blocking!("\tcopy bytes={kernel_copy_len}");
    crate::sendln_blocking!("\tentry={kernel_entry:#?}");

    core::ptr::copy(stub_begin as *const u8, stub_dst, stub_len);

    struct NullLog;
    impl Logger for NullLog {
        fn writeln_fmt(&mut self, reactor: &mut Reactor, args: Arguments) {}
        fn write_fmt(&mut self, reactor: &mut Reactor, args: Arguments) {}
    }

    crate::sendln_blocking!("[device:v1]: Loaded relocation-stub, jumping");

    // flush the logs
    rz.uart_buffer._flush_to_uart1_fifo(&rz.peri.UART1);
    // flush the IR
    io.io_queue_message(rz, &mut NullLog, &v1::device::Booting);
    io.io_flush_blocking(rz);
    // let io_any : &mut dyn Any = io.into();
    // let io = io_any.downcast_mut::<IrDriver>().unwrap();
    // let _ = io.frame_sink.send(&v1::device::Booting);

    __flush_tx(&rz.peri.UART1);

    core::arch::asm!(
        "bx {t0}",
        in("r0") kernel_dst,
        in("r1") kernel_src,
        in("r2") kernel_copy_len,
        in("r3") kernel_entry,
        t0 = in(reg) stub_dst,
        options(noreturn),
    )
}
