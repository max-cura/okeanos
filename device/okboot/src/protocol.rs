use crate::arch::timing::Instant;
use crate::{arch, stub, timeouts};
use bcm2835_lpa::Peripherals;
use core::time::Duration;
use okboot_common::INITIAL_BAUD_RATE;
use thiserror::Error;

const COBS_ENCODE_BUFFER_SIZE: usize = 255;

pub fn run(peripherals: &mut Peripherals, raw_buffer_config: RawBufferConfig) {
    const _: () = {
        assert!(
            INITIAL_BAUD_RATE == 115200,
            "B115200_DIVIDER adjustment required"
        );
    };
    const B115200_DIVIDER: u16 = 270;
    arch::mini_uart::muart1_init(
        &peripherals.GPIO,
        &peripherals.AUX,
        &peripherals.UART1,
        B115200_DIVIDER,
    );

    let end_of_program = stub::locate_end();
    let buffer_space_start = (end_of_program.addr() + 3) & !3;
    let rb = unsafe { BufferArena::new(buffer_space_start, raw_buffer_config) };
}

#[derive(Debug, Copy, Clone)]
pub struct RawBufferConfig {
    /// Memory (in bytes) to use for the receive buffer.
    pub receive: usize,
    /// Memory (in bytes) to use for the transmit buffer.
    pub transmit: usize,
    pub staging: usize,
}
struct AllocatedBuffers<'a> {
    pub receive_buffer: &'a mut [u8],
    pub transmit_buffer: &'a mut [u8],
    pub staging_buffer: &'a mut [u8],
    pub cobs_encode_buffer: &'a mut [u8],
}
struct BufferArena {
    buffers: Option<AllocatedBuffers<'static>>,

    exposed_base: *mut u8,
    unsafe_end_of_buffers: *const (),
    pub unsafe_memory_ends: *const (),
}
impl BufferArena {
    unsafe fn new(base: usize, config: RawBufferConfig) -> Self {
        let exposed_base: *mut u8 = core::ptr::with_exposed_provenance_mut(base);
        let required_memory =
            config.receive + config.transmit + config.staging + COBS_ENCODE_BUFFER_SIZE;
        let receive_buffer_ptr = exposed_base;
        let transmit_buffer_ptr = exposed_base.add(config.receive);
        let staging_buffer_ptr = transmit_buffer_ptr.add(config.transmit);
        let cobs_buffer_ptr = staging_buffer_ptr.add(config.staging);
        let receive_buffer = core::slice::from_raw_parts_mut(receive_buffer_ptr, config.receive);
        let transmit_buffer = core::slice::from_raw_parts_mut(transmit_buffer_ptr, config.transmit);
        let staging_buffer = core::slice::from_raw_parts_mut(staging_buffer_ptr, config.staging);
        let cobs_encode_buffer =
            core::slice::from_raw_parts_mut(cobs_buffer_ptr, COBS_ENCODE_BUFFER_SIZE);
        Self {
            buffers: Some(AllocatedBuffers {
                receive_buffer,
                transmit_buffer,
                staging_buffer,
                cobs_encode_buffer,
            }),
            exposed_base,
            unsafe_end_of_buffers: exposed_base.add(required_memory).cast(),
            unsafe_memory_ends: core::ptr::without_provenance(512 * 1024 * 1024),
        }
    }

    pub fn take(&mut self) -> Option<AllocatedBuffers> {
        self.buffers.take()
    }
}

#[derive(Debug, Error, Copy, Clone)]
#[non_exhaustive]
enum ReceiveError {
    #[error("incoming message overflowed receive buffer")]
    BufferOverflow,
    #[error("incoming message overran the FIFO")]
    FifoOverrun,
    #[error("protocol error")]
    Protocol,
    #[error("error decoding message")]
    Decode,
}

#[derive(Debug, Copy, Clone)]
struct ErrorState {
    at_instant: Instant,
    receive_error: Option<ReceiveError>,
}

#[derive(Debug, Copy, Clone)]
struct Timeouts {
    error_recovery: Duration,
    byte_read: Duration,
    session_expires: Duration,
}
impl Timeouts {
    pub fn new_8n1(baud: u32) -> Timeouts {
        Self {
            error_recovery: timeouts::ERROR_RECOVERY.at_baud_8n1(baud),
            byte_read: timeouts::BYTE_READ.at_baud_8n1(baud),
            session_expires: timeouts::SESSION_EXPIRES.at_baud_8n1(baud),
        }
    }
}

#[derive(Debug)]
pub enum ProtocolStatus {
    Continue,
    Abcon,
    Abend,
}
