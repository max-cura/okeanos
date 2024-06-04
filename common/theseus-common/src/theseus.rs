pub mod handshake;
pub mod len;
pub mod v1;

/// A shortened version of Ethernet's preamble.
pub const PREAMBLE: u32 = 0x5e555555;

pub type MessageTypeType = u32;

/// MSG_PRINT_STRING is special: it's just <PREAMBLE> <LENGTH> <[c1 c0 c0 c0]> <COBS [..data..]>
/// where data is simply a string.
/// This is due to a limitation in postcard: strings and byte arrays write the length as a
/// varint(usize), so when we're doing format_args! or anything of the sort, we cannot know, ahead
/// of time, the length of the string, and thus we cannot know the size of the slot required for the
/// `len` field of the string/slice.
pub const MSG_PRINT_STRING: MessageTypeType = 1;

pub trait MessageClass {
    const MSG_TYPE: MessageTypeType;
}
