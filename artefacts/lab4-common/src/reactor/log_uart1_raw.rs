use core::fmt::{self, Write as _};
use crate::reactor::{Logger, Reactor};

#[derive(Debug)]
pub struct RawUart1Logger;

impl Logger for RawUart1Logger {
    fn writeln_fmt(&mut self, reactor: &mut Reactor, args: core::fmt::Arguments) {
        let _ = reactor.uart_buffer.write_fmt(args);
        reactor.uart_buffer.push_byte(b'\n');
    }
    fn write_fmt(&mut self, reactor: &mut Reactor, args: core::fmt::Arguments) {
        let _ = reactor.uart_buffer.write_fmt(args);
    }
}