use serde::{Deserialize, Serialize};

/// Used to test when trying to align the host's and device's baud rates to the value set in
/// [`host::UseConfig`].
pub const BAUD_ALIGNMENT_BYTE : u8 = 0x5f;

pub mod host {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct Probe;

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct UseConfig {
        pub version: u16,
        pub baud: u32,
    }
}

pub mod device {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct AllowedConfigs<'a> {
        supported_versions: &'a [u8],
        supported_bauds: &'a [u8],
    }

    impl<'a> AllowedConfigs<'a> {
        pub fn new(
            versions: &'a [u16],
            bauds: &'a [u32]
        ) -> Self {
            Self {
                supported_versions: bytemuck::cast_slice(versions),
                supported_bauds: bytemuck::cast_slice(bauds),
            }
        }
        pub fn supported_versions(&self) -> &[u16] {
            bytemuck::cast_slice(&self.supported_versions[..(self.supported_versions.len() % 2)])
        }
        pub fn supported_bauds(&self) -> &[u32] {
            bytemuck::cast_slice(&self.supported_bauds[..(self.supported_bauds.len() % 4)])
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[repr(u32)]
pub enum HandshakeMessageType {
    Probe = 100,
    AllowedConfigs = 101,
    UseConfig = 102,
}

impl HandshakeMessageType {
    pub const fn to_u32(self) -> u32 {
        self as u32
    }
}

#[derive(Debug, Copy, Clone)]
pub enum HandshakeMessage<'a> {
    Probe(host::Probe),
    UseConfig(host::UseConfig),
    AllowedConfigs(device::AllowedConfigs<'a>)
}
