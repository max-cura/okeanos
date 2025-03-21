use crate::net::control::OptFromBytes;
use crate::println;
use alloc::vec::Vec;

#[derive(Debug)]
pub enum Opt<'a> {
    Mru(u16),
    Accm(u32),
    AuthProtocol(u16, &'a [u8]),
    QualProtocol(u16, &'a [u8]),
    Magic(u32),
    PFCompression,
    ACFCompression,
}
impl<'a> Opt<'a> {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        match self {
            Opt::Mru(mru) => {
                v.extend_from_slice(&[1, 4]);
                v.extend_from_slice(&mru.to_be_bytes());
            }
            Opt::Accm(accm) => {
                v.extend_from_slice(&[2, 6]);
                v.extend_from_slice(&accm.to_be_bytes());
            }
            Opt::AuthProtocol(_, _) => {
                panic!("not supported outbound: Authentication-Protocol");
            }
            Opt::QualProtocol(_, _) => {
                panic!("not supported outbound: Quality-Protocol");
            }
            Opt::Magic(magic) => {
                v.extend_from_slice(&[5, 6]);
                v.extend_from_slice(&magic.to_be_bytes());
            }
            Opt::PFCompression => {
                panic!("not supported outbound: Protocol-Field-Compression");
            }
            Opt::ACFCompression => {
                panic!("not supported outbound: Address-and-Control-Field-Compression");
            }
        }
        v
    }
}

impl<'a> OptFromBytes<'a> for Opt<'a> {
    fn from_bytes(ty: u8, data: &'a [u8]) -> Option<Self>
    where
        Self: Sized,
    {
        let opt_len = data.len() + 2;
        match ty {
            1 => {
                // len == 4
                if opt_len != 4 {
                    println!("lcp: option MRU with length {:02x}", opt_len);
                    return None;
                }
                let mru = u16::from_be_bytes([data[0], data[1]]);
                Some(Opt::Mru(mru))
            }
            2 => {
                // ACCM
                if opt_len != 6 {
                    println!("lcp: option ACCM with length {:02x}", opt_len);
                    return None;
                }
                let accm = u32::from_be_bytes(data.try_into().unwrap());
                Some(Opt::Accm(accm))
            }
            3 => {
                // Auth
                if data.len() < 4 {
                    println!("lcp: option AP with length {:02x}", opt_len);
                    return None;
                }
                let proto = u16::from_be_bytes([data[0], data[1]]);
                Some(Opt::AuthProtocol(proto, &data[2..]))
            }
            4 => {
                // Qual
                if data.len() < 4 {
                    println!("lcp: option QP with length {:02x}", opt_len);
                    return None;
                }
                let proto = u16::from_be_bytes([data[0], data[1]]);
                Some(Opt::QualProtocol(proto, &data[2..]))
            }
            5 => {
                // Magic
                if opt_len != 6 {
                    println!("lcp: option MN with length {:02x}", opt_len);
                    return None;
                }
                let magic = u32::from_be_bytes(data.try_into().unwrap());
                Some(Opt::Magic(magic))
            }
            7 => {
                // Protocol Field Compression
                if opt_len != 2 {
                    println!("lcp: option PFC with length {:02x}", opt_len);
                    return None;
                }
                Some(Opt::PFCompression)
            }
            8 => {
                // Address and Control Field Compression
                if opt_len != 2 {
                    println!("lcp: option ACFC with length {:02x}", opt_len);
                    return None;
                }
                Some(Opt::ACFCompression)
            }
            x => {
                println!("lcp: unknown option {x:02x}");
                None
            }
        }
    }
}
