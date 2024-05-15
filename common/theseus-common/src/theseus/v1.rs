use serde::{Serialize, Deserialize};
use crate::theseus::MessageTypeType;

#[derive(Debug, Deserialize, Serialize)]
#[repr(u32)]
pub enum V1MessageType {
    RequestProgramInfo = 200,
    ProgramInfo = 201,
    RequestProgram = 202,
    ProgramReady = 203,

    RequestChunk = 300,
    Chunk = 301,

    Booting = 400,
}

impl V1MessageType {
    pub const fn to_u32(self) -> u32 {
        self as u32
    }
}

pub const MSG_REQUEST_PROGRAM_INFO : MessageTypeType = V1MessageType::RequestProgramInfo.to_u32();
pub const MSG_PROGRAM_INFO : MessageTypeType = V1MessageType::ProgramInfo.to_u32();
pub const MSG_REQUEST_PROGRAM : MessageTypeType = V1MessageType::RequestProgram.to_u32();
pub const MSG_PROGRAM_READY : MessageTypeType = V1MessageType::ProgramReady.to_u32();
pub const MSG_REQUEST_CHUNK : MessageTypeType = V1MessageType::RequestChunk.to_u32();
pub const MSG_CHUNK : MessageTypeType = V1MessageType::Chunk.to_u32();
pub const MSG_BOOTING : MessageTypeType = V1MessageType::Booting.to_u32();

pub mod host {
    use serde::{Deserialize, Serialize};
    use crate::theseus::{MessageClass, MessageTypeType};

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct ProgramInfo {
        pub load_at_addr: u32,

        pub compressed_len: u32,
        pub decompressed_len: u32,

        pub compressed_crc: u32,
        pub decompressed_crc: u32,
    }
    
    impl MessageClass for ProgramInfo {
        const MSG_TYPE: MessageTypeType = super::MSG_PROGRAM_INFO;
    }

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct ProgramReady;

    impl MessageClass for ProgramReady {
        const MSG_TYPE: MessageTypeType = super::MSG_PROGRAM_READY;
    }

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct Chunk<'a> {
        pub chunk_no: u32,
        pub data: &'a [u8],
    }

    impl<'a> MessageClass for Chunk<'a> {
        const MSG_TYPE: MessageTypeType = super::MSG_CHUNK;
    }
}

pub mod device {
    use serde::{Deserialize, Serialize};
    use crate::theseus::{MessageClass, MessageTypeType};

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct RequestProgramInfo;

    impl MessageClass for RequestProgramInfo {
        const MSG_TYPE: MessageTypeType = super::MSG_REQUEST_PROGRAM_INFO;
    }

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct RequestProgram {
        pub chunk_size: u32,
        pub verify_compressed_crc: u32,
        pub verify_decompressed_crc: u32,
    }

    impl MessageClass for RequestProgram {
        const MSG_TYPE: MessageTypeType = super::MSG_REQUEST_PROGRAM;
    }

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct RequestChunk {
        pub chunk_no: u32
    }

    impl MessageClass for RequestChunk {
        const MSG_TYPE: MessageTypeType = super::MSG_REQUEST_CHUNK;
    }

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct Booting;

    impl MessageClass for Booting {
        const MSG_TYPE: MessageTypeType = super::MSG_BOOTING;
    }
}