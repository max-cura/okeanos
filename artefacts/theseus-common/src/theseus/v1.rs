use serde::{Serialize, Deserialize};

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

pub mod host {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct ProgramInfo {
        compressed_len: u32,
        decompressed_len: u32,

        compressed_crc: u32,
        decompressed_crc: u32,
    }

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct ProgramReady;

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct Chunk<'a> {
        chunk_no: u32,
        data: &'a [u8],
    }
}

pub mod device {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct RequestProgramInfo;

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct RequestProgram {
        chunk_size: u32,
        verify_compressed_crc: u32,
        verify_decompressed_crc: u32,
    }

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct RequestChunk {
        chunk_no: u32
    }

    #[derive(Debug, Copy, Clone, Serialize, Deserialize)]
    pub struct Booting;
}