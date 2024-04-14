use core::fmt::{Display, Formatter};
use serde::{Deserialize, Serialize};

/// Format is `1100 1100 1110 1110 xxxx yyyy zzzz 1010` where the first `xxxx` is the version,
/// and `yyyy=(xxxx+3) mod 16`, `zzzz=(yyyy+3) mod 16`.
/// ```txt
/// xxxx
/// -----
/// 0000 - reserved
/// 0001 - Theseus v1
/// 0010:1110 - unassigned
/// 1111 - reserved
/// ```
#[derive(Debug, Copy, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[repr(u32)]
pub enum TheseusVersion {
    TheseusV1 = 0xCCEE147A,
}

impl TheseusVersion {
    pub const fn max_value() -> Self {
        Self::TheseusV1
    }
}

impl Display for TheseusVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            TheseusVersion::TheseusV1 => write!(f, "THESEUSv1"),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum VersionValidation {
    ValidUnknown,
    Invalid,
}

pub fn validate_version(v: u32) -> Result<TheseusVersion, VersionValidation> {
    if (v & 0xffff000f) != 0xccee000a {
        return Err(VersionValidation::Invalid);
    }
    let x1 = (v & 0x0000f000) >> 12;
    let x2 = (v & 0x00000f00) >> 8;
    let x3 = (v & 0x000000f0) >> 4;

    fn plus3mod16(x: u32) -> u32 {
        (x + 3) % 16
    }

    if x2 != plus3mod16(x1) || x3 != plus3mod16(x2) {
        return Err(VersionValidation::Invalid);
    }

    match x1 {
        1 => Ok(TheseusVersion::TheseusV1),
        2..=14 => Err(VersionValidation::ValidUnknown),
        // ???
        0 | 15 => Err(VersionValidation::ValidUnknown),
        _ => unreachable!("16-bit value cannot be outside the range 0..=15"),
    }
}

pub mod v1;

#[derive(Debug, Clone)]
pub struct ProgramCRC32 {
    inner: crc32fast::Hasher,
}

impl ProgramCRC32 {
    pub fn new() -> Self {
        Self { inner: crc32fast::Hasher::new() }
    }
    pub fn add_data(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }
    pub fn finalize(self) -> u32 {
        self.inner.finalize()
    }
}

impl Default for ProgramCRC32 {
    fn default() -> Self {
        Self::new()
    }
}
