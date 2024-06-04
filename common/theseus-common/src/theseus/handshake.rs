use serde::{Deserialize, Serialize};

/// Used to test when trying to align the host's and device's baud rates to the value set in
/// [`host::UseConfig`].
pub const BAUD_ALIGNMENT_BYTE: u8 = 0x5f;

pub mod host {
    use crate::theseus::{MessageClass, MessageTypeType};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct Probe;

    impl MessageClass for Probe {
        const MSG_TYPE: MessageTypeType = super::MSG_PROBE;
    }

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct UseConfig {
        pub version: u16,
        pub baud: u32,
    }
    impl MessageClass for UseConfig {
        const MSG_TYPE: MessageTypeType = super::MSG_USE_CONFIG;
    }
}

pub mod device {
    use crate::theseus::{MessageClass, MessageTypeType};
    use serde::{Deserialize, Serialize};

    // opaque type (reason: alignment)
    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct AllowedConfigs<'a> {
        supported_versions: &'a [u8],
        supported_bauds: &'a [u8],
    }

    impl<'a> AllowedConfigs<'a> {
        pub fn new(versions: &'a [u16], bauds: &'a [u32]) -> Self {
            Self {
                supported_bauds: bytemuck::cast_slice(bauds),
                supported_versions: bytemuck::cast_slice(versions),
            }
        }
    }
    impl<'a> MessageClass for AllowedConfigs<'a> {
        const MSG_TYPE: MessageTypeType = super::MSG_ALLOWED_CONFIGS;
    }

    #[cfg(feature = "std")]
    #[derive(Debug, Clone)]
    pub struct AllowedConfigsHelper {
        pub supported_versions: Vec<u16>,
        pub supported_bauds: Vec<u32>,
    }
    #[cfg(feature = "std")]
    impl<'a> From<AllowedConfigs<'a>> for AllowedConfigsHelper {
        fn from(value: AllowedConfigs<'a>) -> Self {
            let mut b1 = vec![0u16; value.supported_versions.len() / 2];
            let mut b2 = vec![0u32; value.supported_bauds.len() / 4];
            bytemuck::cast_slice_mut::<u16, u8>(&mut b1).copy_from_slice(value.supported_versions);
            bytemuck::cast_slice_mut::<u32, u8>(&mut b2).copy_from_slice(value.supported_bauds);
            AllowedConfigsHelper {
                supported_versions: b1,
                supported_bauds: b2,
            }
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

pub const MSG_PROBE: u32 = HandshakeMessageType::Probe.to_u32();
pub const MSG_ALLOWED_CONFIGS: u32 = HandshakeMessageType::AllowedConfigs.to_u32();
pub const MSG_USE_CONFIG: u32 = HandshakeMessageType::UseConfig.to_u32();

impl HandshakeMessageType {
    pub const fn to_u32(self) -> u32 {
        self as u32
    }
}

#[derive(Debug, Copy, Clone)]
pub enum HandshakeMessage<'a> {
    Probe(host::Probe),
    UseConfig(host::UseConfig),
    AllowedConfigs(device::AllowedConfigs<'a>),
}
