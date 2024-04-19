pub mod v1;
pub mod handshake;

/// A shortened version of Ethernet's preamble.
pub const PREAMBLE : u32 = 0x5e555555;

pub type MessageTypeType = u32;
