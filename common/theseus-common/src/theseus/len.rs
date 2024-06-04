pub fn encode_len(len: usize) -> [u8; 4] {
    if len >= (1 << 24) {
        [0xc0, 0xc0, 0xc0, 0xc0]
    } else {
        let len = (len & 0x00ffffff) as u32;
        let b0 = (len & 0x3f) | 0xc0;
        let b1 = ((len & 0xfc0) >> 6) | 0xc0;
        let b2 = ((len & 0x3f000) >> 12) | 0xc0;
        let b3 = ((len & 0xfc0000) >> 18) | 0xc0;
        [b0 as u8, b1 as u8, b2 as u8, b3 as u8]
    }
}

pub fn decode_len(bytes: &[u8]) -> u32 {
    if bytes.len() != 4 || bytes.iter().any(|x| (x & 0xc0) != 0xc0) {
        0
    } else {
        let mut b = 0;
        b |= (bytes[0] & 0x3f) as u32;
        b |= ((bytes[1] & 0x3f) as u32) << 6;
        b |= ((bytes[2] & 0x3f) as u32) << 12;
        b |= ((bytes[3] & 0x3f) as u32) << 18;
        b
    }
}
