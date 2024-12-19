#![allow(internal_features)]
#![feature(core_intrinsics)]
#![cfg_attr(not(feature = "std"), no_std)]
//! Common structures shared by the `okboot` and `okup` crates.

use musli::{Decode, Encode};

/// The baud rate at which the protocol is run.
pub const BAUD_RATE: usize = 1_500_000;

/// Message preamble, shortened from Ethernet.
pub const PREAMBLE_BYTES: [u8; 4] = [0x55, 0x55, 0x55, 0x5e];

/// Message structures sent from the device.
pub mod device;
/// Frame encoding and decoding, for both sides.
pub mod frame;
/// Message structure sent from the host.
pub mod host;

/// Message type enumeration.
#[derive(Debug, Encode, Decode, Clone, Copy)]
#[repr(u32)]
pub enum MessageType {
    /// Corresponds to [`PrintString`](device::PrintString).
    PrintString = 101,
    /// Corresponds to [`Probe`](host::Probe)
    Probe = 201,
    /// Corresponds to [`AllowedVersions`](device::AllowedVersions).
    AllowedVersions = 202,
    /// Corresponds to [`UseVersion`](host::UseVersion)
    UseVersion = 203,
    /// Corresponds to [`MetadataReq`](device::MetadataReq)
    MetadataReq = 301,
    /// Corresponds to [`Metadata`](host::Metadata)
    Metadata = 302,
    /// Corresponds to [`MetadataAck`](device::MetadataAck)
    MetadataAck = 303,
    /// Corresponds to [`MetadataAckAck`](host::MetadataAckAck)
    MetadataAckAck = 304,
    /// Corresponds to [`ChunkReq`](device::ChunkReq`)
    ChunkReq = 401,
    /// Corresponds to [`Chunk`](host::Chunk)
    Chunk = 402,
    /// Corresponds to [`Booting`](device::Booting)
    Booting = 501,
    /// Corresponds to [`BootingAck`](host::BootingAck)
    BootingAck = 502,
}
impl TryFrom<u32> for MessageType {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(match value {
            101 => Self::PrintString,
            201 => Self::Probe,
            202 => Self::AllowedVersions,
            203 => Self::UseVersion,
            301 => Self::MetadataReq,
            302 => Self::Metadata,
            303 => Self::MetadataAck,
            304 => Self::MetadataAckAck,
            401 => Self::ChunkReq,
            402 => Self::Chunk,
            501 => Self::Booting,
            502 => Self::BootingAck,
            _ => return Err(()),
        })
    }
}
