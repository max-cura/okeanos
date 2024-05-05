#![cfg_attr(not(feature = "std"), no_std)]

pub const INITIAL_BAUD_RATE: u32 = 115200;

pub mod theseus;
pub mod cobs;

pub mod su_boot {
    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    #[repr(u32)]
    pub enum Command {
        BootStart = 0xFFFF0000,

        GetProgInfo = 0x11112222, // pi sends
        PutProgInfo = 0x33334444,       // unix sends

        GetCode = 0x55556666, // pi sends
        PutCode = 0x77778888,       // unix sends

        BootSuccess = 0x9999AAAA, // pi sends on success
        BootError = 0xBBBBCCCC,       // pi sends on failure.

        PrintString = 0xDDDDEEEE,       // pi sends to print a string.
    }
}
