use musli::{Decode, Encode};

/// Indicates to the device that a host has arrived and that it should broadcast an
/// [`AllowedVersions`](crate::device::AllowedVersions).
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct Probe {}

/// Indicates which version of the protocol should be used.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct UseVersion {
    pub version: u32,
}

/// Metadata that the device needs to know about the program being downloaded.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct Metadata {
    pub deflated_crc: u32,
    pub deflated_len: u32,
    pub inflated_crc: u32,
    pub inflated_len: u32,
}

/// Signals that the [`MetadataAck`](crate::device::MetadataAck) was received, and additionally
/// indicates whether the `MetadataAck` was correct.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct MetadataAckAck {
    is_ok: bool,
}

/// A chunk of program data that is being uploaded to the device.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct Chunk<'a> {
    pub which: u32,
    pub bytes: &'a [u8],
}

/// Signals that the [`Booting`](crate::device::Booting) was received.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct BootingAck {}
