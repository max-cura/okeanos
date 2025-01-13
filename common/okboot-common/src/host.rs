use crate::{EncodeMessageType, MessageType};
use musli::{Decode, Encode};

/// Indicates to the device that a host has arrived and that it should broadcast an
/// [`AllowedVersions`](crate::device::AllowedVersions).
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct Probe {}
impl EncodeMessageType for Probe {
    const TYPE: MessageType = MessageType::Probe;
}

/// Indicates which version of the protocol should be used.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct UseVersion {
    pub version: u32,
}
impl EncodeMessageType for UseVersion {
    const TYPE: MessageType = MessageType::UseVersion;
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
impl EncodeMessageType for Metadata {
    const TYPE: MessageType = MessageType::Metadata;
}

/// Signals that the [`MetadataAck`](crate::device::MetadataAck) was received, and additionally
/// indicates whether the `MetadataAck` was correct.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct MetadataAckAck {
    is_ok: bool,
}
impl EncodeMessageType for MetadataAckAck {
    const TYPE: MessageType = MessageType::MetadataAck;
}

/// A chunk of program data that is being uploaded to the device.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct Chunk<'a> {
    pub which: u32,
    pub bytes: &'a [u8],
}
impl EncodeMessageType for Chunk<'_> {
    const TYPE: MessageType = MessageType::Chunk;
}

/// Signals that the [`Booting`](crate::device::Booting) was received.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct BootingAck {}
impl EncodeMessageType for BootingAck {
    const TYPE: MessageType = MessageType::BootingAck;
}
