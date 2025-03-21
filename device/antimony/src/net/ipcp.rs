use crate::net::control::OptFromBytes;
use crate::println;
use alloc::vec::Vec;
use core::net::Ipv4Addr;

#[derive(Debug)]
pub enum Opt<'a> {
    IpCompressionProtocol { protocol: u16, data: &'a [u8] },
    IpAddress { ip_addr: Ipv4Addr },
}
impl<'a> Opt<'a> {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::new();
        match self {
            Opt::IpCompressionProtocol { protocol, data } => {
                panic!("not supported outbound: IP-Compression-Protocol");
            }
            Opt::IpAddress { ip_addr } => {
                v.extend_from_slice(&[3, 6]);
                v.extend_from_slice(&ip_addr.octets());
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
                println!("ipcp: IP-Addresses is deprecated");
                None
            }
            2 => {
                if opt_len < 4 {
                    println!("ipcp: option IP-Compression with length {opt_len:02x}");
                    return None;
                }
                let protocol = u16::from_be_bytes([data[0], data[1]]);
                Some(Opt::IpCompressionProtocol {
                    protocol,
                    data: &data[4..],
                })
            }
            3 => {
                if opt_len != 6 {
                    println!("ipcp: option IP-Address with length {opt_len:02x}");
                    return None;
                }
                Some(Opt::IpAddress {
                    ip_addr: Ipv4Addr::from_octets([data[0], data[1], data[2], data[3]]),
                })
            }
            x => {
                println!("ipcp: unknown option {x:02x}");
                None
            }
        }
    }
}
