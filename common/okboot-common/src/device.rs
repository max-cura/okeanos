use musli::{Decode, Encode};

/// Send a string to the host to be printed out. Messages will be line-buffered in a timeout-limited
/// manner.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct PrintString<'a> {
    pub string: &'a str,
}

/// Indicates the protocol versions that the device can speak.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct AllowedVersions<'a> {
    pub versions: &'a [u32],
}

/// Signals that the host should send program [`Metadata`](crate::host::Metadata).
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct MetadataReq {}

/// Retransmission (for confirmation) of metadata information, and the chunk size that the host
/// should use for transmission.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct MetadataAck {
    pub chunk_size: u32,
    pub metadata: crate::host::Metadata,
}

/// Request a specific chunk from the host.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct ChunkReq {
    pub which: u32,
}

/// Indicate that the device has finished downloading.
#[derive(Debug, Encode, Decode)]
#[repr(C)]
pub struct Booting {}
