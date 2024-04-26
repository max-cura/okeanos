use theseus_common::{
    cobs::{BufferedEncoder, EncodeState},
    theseus::{self, MessageClass},
};

pub trait HostEncode : serde::Serialize + MessageClass {
    fn encode(&self) -> color_eyre::Result<Vec<u8>> {
        let buf = vec![];
        let buf = postcard::to_extend(&Self::MSG_TYPE, buf)?;
        let buf = postcard::to_extend(self, buf)?;
        Ok(buf)
    }
}

pub fn frame_bytes(bytes: &[u8]) -> color_eyre::Result<Vec<u8>> {
    let mut frame = vec![];

    frame.extend_from_slice(&theseus::PREAMBLE.to_le_bytes());

    let bytes_crc = crc32fast::hash(bytes);

    let cobs_frame = {
        let mut cobs_frame = vec![];

        let mut cobs_buffer = [0u8; 255];
        let mut cobs_encoder = BufferedEncoder::with_buffer(&mut cobs_buffer, 0x55).unwrap();
        let mut packet = cobs_encoder.packet();
        for &b in bytes.iter().chain(bytes_crc.to_le_bytes().iter()) {
            match packet.add_byte(b) {
                EncodeState::Buf(buf) => {
                    cobs_frame.extend_from_slice(buf);
                }
                EncodeState::Pass => {}
            }
        }
        cobs_frame.extend_from_slice(packet.finish());
        // cobs_frame.iter_mut().for_each(|b| *b = *b ^ 0x55);
        cobs_frame
    };

    // frame.extend_from_slice(postcard::to_stdvec(&(cobs_frame.len() as u32)).unwrap().as_slice());
    frame.extend_from_slice(&theseus::len::encode_len(cobs_frame.len()));
    frame.extend_from_slice(&cobs_frame);

    Ok(frame)
}
