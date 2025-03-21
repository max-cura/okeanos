use crate::net::phy::send;
use crate::net::{Buffer, PPP_MRU, PROTO_LCP, hexdump};
use crate::println;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::net::Ipv4Addr;

/// LCP-like protocols

#[derive(Debug, Copy, Clone)]
pub struct Packet<'a> {
    pub code: Code,
    pub id: u8,
    pub data: &'a [u8],
}
impl<'a> Packet<'a> {
    pub fn new(code: Code, id: u8, data: &'a [u8]) -> Self {
        Self { code, id, data }
    }
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Code {
    ConfReq = 1,
    ConfAck = 2,
    ConfNak = 3,
    ConfRej = 4,

    TermReq = 5,
    TermAck = 6,

    CodeRej = 7,
}
impl TryFrom<u8> for Code {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::ConfReq),
            2 => Ok(Self::ConfAck),
            3 => Ok(Self::ConfNak),
            4 => Ok(Self::ConfRej),
            5 => Ok(Self::TermReq),
            6 => Ok(Self::TermAck),
            7 => Ok(Self::CodeRej),
            _ => Err(()),
        }
    }
}

pub trait OptFromBytes<'a> {
    fn from_bytes(ty: u8, data: &'a [u8]) -> Option<Self>
    where
        Self: Sized;
}
pub struct Options<'a, CP> {
    buf: &'a [u8],
    index: usize,
    _pd: PhantomData<CP>,
}
impl<'a, CP> Options<'a, CP> {
    pub fn from_slice(inner: &'a [u8]) -> Self {
        Self {
            buf: inner,
            index: 0,
            _pd: Default::default(),
        }
    }
}
impl<'a, CP: ControlProtocol> Iterator for Options<'a, CP> {
    type Item = Result<(CP::Opt<'a>, &'a [u8]), &'a [u8]>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.buf.len() {
            return None;
        }
        let ty = self.buf[self.index];
        let len = self.buf[self.index + 1] as usize;
        if self.index + len > self.buf.len() {
            println!(
                "{name}: option overflow: {len:02x}, remaining: {:02x} bytes",
                self.buf.len() - self.index,
                name = CP::name(),
            );
            return Some(Err(&[]));
        }
        let data = &self.buf[self.index + 2..self.index + len];
        let opt_buf = &self.buf[self.index..self.index + len];
        let res = CP::Opt::from_bytes(ty, data)
            .map(|opt| (opt, opt_buf))
            .ok_or(opt_buf);
        self.index += len;
        Some(res)
    }
}

#[derive(Debug)]
pub enum Verdict {
    /// Can accept options as-is
    Ack,
    /// Need different values
    Nak(Vec<u8>),
    /// Cannot accept these options
    Rej(Vec<u8>),
}
impl Verdict {
    pub fn is_reject(&self) -> bool {
        matches!(self, Self::Rej(_))
    }
    pub fn as_reject(&self) -> Option<&[u8]> {
        match self {
            Self::Rej(v) => Some(v),
            _ => None,
        }
    }
    pub fn is_nak(&self) -> bool {
        matches!(self, Self::Nak(_))
    }
    pub fn as_nak(&self) -> Option<&[u8]> {
        match self {
            Self::Nak(v) => Some(v),
            _ => None,
        }
    }
}

pub trait ControlProtocol {
    const PROTOCOL: u16;
    type Opt<'a>: OptFromBytes<'a>;

    fn name() -> &'static str;
    fn received_nak_opt(&mut self, opt: Self::Opt<'_>) -> bool;
    fn received_unknown_code(&mut self, code: u8, id: u8, data: &[u8], s: S);
    fn judge<'a>(&mut self, options: Self::Opt<'a>, buf: &'a [u8]) -> Verdict;
    fn get_opts(&mut self) -> Vec<u8>;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum S {
    /// Not quite RFC-compliant: we assume that we have in perpetuity an administrative Open active,
    /// and that the link is always Up. Furthermore, we don't particularly care about clean
    /// termination (or at least have no reason to at present), so we have a singular Closed state
    /// that takes on the roles of Initial, Starting, Closed, Stopped, Closing, and Stopping.
    /// The Restart timer is not running, and nothing has been sent or received.
    Closed,
    /// A Configure-Request has been sent and the Restart timer is running, but no Configure-Ack has
    /// been received nor has one been sent.
    ReqSent,
    /// A Configure-Request has been sent and a Configure-Ack has been received. The Restart timer
    /// is still running, since a Configure-Ack has not yet been sent.
    AckReceived,
    /// A Configure-Request and a Configure-Ack have both been sent, but a Configure-Ack has not yet
    /// been received. The Restart timer is still running, since a Configure-Ack has not yet been
    /// received.
    AckSent,
    /// A Configure-Ack has been both sent and received. The Restart timer is not running.
    Opened,
}

// We're very noncompliant: we don't resend messages
// struct Timeout {
//     restart_count: usize,
//     restart_timer_started: Instant,
// }
pub struct ControlAutomaton<CP: ControlProtocol> {
    inner: CP,
    state: S,
    scr_id: u8,
    last_req_id: Option<u8>, // timeouts: Option<Timeout>,
}
impl<CP: ControlProtocol> ControlAutomaton<CP> {
    pub fn new(np: CP) -> Self {
        Self {
            inner: np,
            state: S::Closed,
            scr_id: 0,
            last_req_id: None,
        }
    }

    pub fn layer(&self) -> &CP {
        &self.inner
    }
    pub fn layer_mut(&mut self) -> &mut CP {
        &mut self.inner
    }

    pub fn is_layer_up(&self) -> bool {
        self.state == S::Opened
    }
    pub fn is_closed(&self) -> bool {
        self.state == S::Closed
    }

    pub fn received(&mut self, packet: &[u8]) {
        let Some(packet) = self.decode_packet(packet) else {
            return;
        };
        println!("{name}: < {packet:?}", name = CP::name());
        match (packet.code, self.state) {
            (_, S::Closed) => {
                self.send_terminate_ack(packet.id);
            }

            (Code::ConfReq, state) => {
                // We received a Configure-Request from the peer
                let did_ack = self.received_configure_request(packet);
                // By cases:
                //  - Req-Sent (RCR+ -> Ack-Sent)
                //  - Ack-Rcvd (RCR+ -> Opened)
                //  - Ack-Sent (RCR- -> Req-Sent)
                //  - Opened -> TLD, SCR -> (RCR+ -> Ack-Sent, RCR- -> Req-Sent)
                match (did_ack, state) {
                    (true, S::ReqSent) => self.set_state(S::AckSent),
                    (true, S::AckReceived) => self.set_state(S::Opened),
                    (false, S::AckSent) => self.set_state(S::ReqSent),
                    (_, S::Opened) => {
                        if self.last_req_id != Some(packet.id) {
                            self.set_state(if did_ack { S::AckSent } else { S::ReqSent });
                            self.send_configure_request();
                        }
                    }
                    _ => { /* ignore */ }
                }

                self.last_req_id = Some(packet.id);
            }

            (Code::ConfAck, state) => {
                // By cases:
                //  - Req-Sent -> IRC -> Ack-Rcvd
                //  - Ack-Rcvd -> SCR -> (X) -> Req-Sent
                //  - Ack-Sent -> IRC, TLU -> Opened
                //  - Opened -> TLD, SCR -> (X) -> Req-Sent
                if state == S::ReqSent {
                    self.set_state(S::AckReceived)
                } else if state == S::AckSent {
                    self.set_state(S::Opened)
                } else {
                    self.set_state(S::ReqSent);
                    self.send_configure_request();
                }
            }

            (Code::TermReq, state) => {
                self.send_terminate_ack(packet.id);
                if state == S::Opened {
                    self.set_state(S::Closed);
                    self.last_req_id = None;
                } else {
                    self.set_state(S::ReqSent);
                }
            }

            (Code::TermAck, _) => {
                println!(
                    "{name}: we commit no heresy against infinity",
                    name = CP::name()
                );
            }

            (Code::ConfNak, _) => {
                let mut options: Options<'_, CP> = Options::from_slice(packet.data);
                let mut all_ok = true;
                for opt in options {
                    match opt {
                        Ok((opt, _)) => {
                            if !self.inner.received_nak_opt(opt) {
                                all_ok = false;
                            }
                        }
                        Err(_) => {}
                    }
                }
                if all_ok {
                    self.send_configure_request();
                } else {
                    println!(
                        "{name}: couldn't handle this lack of acknowledgement",
                        name = CP::name()
                    );
                }
            }
            (Code::ConfRej | Code::CodeRej, _) => {
                println!("{name}: can't handle rejection", name = CP::name());
            }

            (code, state) => {
                println!(
                    "{name}: unexpected code {code:?} in state {state:?}",
                    name = CP::name()
                );
            }
        }
    }

    fn send_configure_request(&mut self) {
        let opts = self.inner.get_opts();
        let id = self.scr_id();
        self.send(Packet::new(Code::ConfReq, id, &opts));
    }

    fn scr_id(&mut self) -> u8 {
        let id = self.scr_id;
        self.scr_id = self.scr_id.wrapping_add(1);
        id
    }

    /// Returns true if the packet was an ack
    fn received_configure_request(&mut self, packet: Packet) -> bool {
        let mut options: Options<'_, CP> = Options::from_slice(packet.data);
        let Ok(verdicts): Result<Vec<Verdict>, ()> = options
            .map(|opt| match opt {
                Ok((opt, buf)) => Ok(self.inner.judge(opt, buf)),
                Err(s) if s.is_empty() => {
                    // malformed; silently discard
                    Err(())
                }
                Err(s) => {
                    // rejected: unknown option
                    Ok(Verdict::Rej(s.to_vec()))
                }
            })
            .try_collect()
        else {
            // malformed ; ignore silently
            return false;
        };
        if verdicts.iter().any(Verdict::is_reject) {
            self.send_configure_reject(packet.id, verdicts);
            false
        } else if verdicts.iter().any(Verdict::is_nak) {
            self.send_configure_nak(packet.id, verdicts);
            false
        } else {
            self.send_configure_ack(packet);
            true
        }
    }

    fn send_configure_ack(&mut self, mut packet: Packet) {
        // really quite simple
        packet.code = Code::ConfAck;
        self.send(packet);
    }

    fn send_configure_nak(&mut self, id: u8, verdicts: Vec<Verdict>) {
        let mut frame = Buffer::new();
        frame.begin_ppp(CP::PROTOCOL);
        let packet_length = 4 + verdicts
            .iter()
            .filter_map(|v| match v {
                Verdict::Nak(nak) => Some(nak.len() as u16),
                _ => None,
            })
            .sum::<u16>();
        frame.write(&[Code::ConfNak as u8, id]);
        frame.write(&packet_length.to_be_bytes());
        for nak in verdicts.iter().filter_map(Verdict::as_nak) {
            frame.write(nak);
        }
        frame.finish_ppp();
        send(frame);
    }

    fn send_configure_reject(&mut self, id: u8, verdicts: Vec<Verdict>) {
        let mut frame = Buffer::new();
        frame.begin_ppp(CP::PROTOCOL);
        let packet_length = 4 + verdicts
            .iter()
            .filter_map(|v| match v {
                Verdict::Rej(rej) => Some(rej.len() as u16),
                _ => None,
            })
            .sum::<u16>();
        frame.write(&[Code::ConfRej as u8, id]);
        frame.write(&packet_length.to_be_bytes());
        for reject in verdicts.iter().filter_map(Verdict::as_reject) {
            frame.write(reject);
        }
        frame.finish_ppp();
        send(frame);
    }

    fn send_terminate_ack(&mut self, id: u8) {
        self.send(Packet::new(Code::TermAck, id, &[]));
    }

    fn set_state(&mut self, s: S) {
        println!("{name}: {:?} -> {s:?}", self.state, name = CP::name());
        self.state = s;
    }

    pub fn open(&mut self) {
        assert_eq!(self.state, S::Closed);
        self.set_state(S::ReqSent);
        self.send_configure_request();
    }

    pub fn close(&mut self) {
        // TODO: is this sufficient???
        self.set_state(S::Closed);
    }

    fn send(&self, packet: Packet) {
        let mut frame = Buffer::new();
        frame.begin_ppp(CP::PROTOCOL);
        frame.write(&[packet.code as u8, packet.id]);
        let pkt_len = (4 + packet.data.len()) as u16;
        frame.write(&pkt_len.to_be_bytes());
        frame.write(packet.data);
        frame.finish_ppp();
        send(frame);
    }

    fn decode_packet<'p>(&mut self, packet: &'p [u8]) -> Option<Packet<'p>> {
        // decode packet:
        if packet.len() < 4 {
            println!(
                "{name}: packet too small (length = {len})",
                len = packet.len(),
                name = CP::name(),
            );
            return None;
        }
        let pkt_code = packet[0];
        let pkt_id = packet[1];
        let pkt_len = u16::from_be_bytes([packet[2], packet[3]]);
        let pkt_data = &packet[4..];
        if pkt_len as usize != packet.len() {
            println!(
                "{name}: packet length mismatch (declared = {declared}, actual = {actual})",
                name = CP::name(),
                declared = pkt_len,
                actual = packet.len()
            );
            return None;
        }
        let Ok(pkt_code) = Code::try_from(pkt_code) else {
            self.inner
                .received_unknown_code(pkt_code, pkt_id, pkt_data, self.state);
            return None;
        };
        Some(Packet {
            code: pkt_code,
            id: pkt_id,
            data: pkt_data,
        })
    }
}

pub struct Ipcp {
    pub host_address: Option<Ipv4Addr>,
    pub prev_host_address: Option<Option<Ipv4Addr>>,
    pub peer_address: Option<Ipv4Addr>,
}
impl Ipcp {
    pub fn new() -> Self {
        Self {
            host_address: None,
            prev_host_address: None,
            peer_address: None,
        }
    }
    pub fn set_host_ip(&mut self, new_ip: Ipv4Addr) {
        self.prev_host_address = Some(self.host_address.replace(new_ip));
    }
    pub fn get_host_addr_update(&mut self) -> Option<(Option<Ipv4Addr>, Ipv4Addr)> {
        if let Some(addr) = self.prev_host_address.take() {
            Some((addr, self.host_address.unwrap()))
        } else {
            None
        }
    }
    pub fn peer_addr(&self) -> Option<Ipv4Addr> {
        self.peer_address
    }
}
impl ControlProtocol for Ipcp {
    const PROTOCOL: u16 = 0x8021;
    type Opt<'a> = super::ipcp::Opt<'a>;

    fn name() -> &'static str {
        "ipcp"
    }

    fn received_nak_opt(&mut self, opt: Self::Opt<'_>) -> bool {
        use super::ipcp::Opt;
        match opt {
            Opt::IpCompressionProtocol { .. } => false,
            Opt::IpAddress { ip_addr } => {
                self.set_host_ip(ip_addr);
                true
            }
        }
    }

    fn received_unknown_code(&mut self, code: u8, id: u8, data: &[u8], s: S) {
        println!(
            "ipcp: received {code:02x}:{id:02x}:{} in {s:?}",
            hexdump(data)
        );
    }

    fn judge<'a>(&mut self, opt: Self::Opt<'a>, buf: &'a [u8]) -> Verdict {
        use super::ipcp::Opt;
        match opt {
            Opt::IpCompressionProtocol { .. } => Verdict::Rej(buf.to_vec()),
            Opt::IpAddress { ip_addr } => {
                self.peer_address = Some(ip_addr);
                Verdict::Ack
            }
        }
    }

    fn get_opts(&mut self) -> Vec<u8> {
        super::ipcp::Opt::IpAddress {
            ip_addr: self.host_address.unwrap_or(Ipv4Addr::new(0, 0, 0, 0)),
        }
        .to_bytes()
    }
}

pub struct Lcp;
impl Lcp {
    pub fn new() -> Self {
        Self
    }
}
impl ControlProtocol for Lcp {
    const PROTOCOL: u16 = PROTO_LCP;
    type Opt<'a> = super::lcp::Opt<'a>;
    fn name() -> &'static str {
        "lcp"
    }

    fn received_nak_opt(&mut self, _opt: Self::Opt<'_>) -> bool {
        false
    }

    fn received_unknown_code(&mut self, code: u8, id: u8, data: &[u8], s: S) {
        if code == 8 {
            if data.len() >= 2 {
                let protocol = u16::from_be_bytes([data[0], data[1]]);
                println!("lcp: received Protocol-Reject with protocol {protocol:04x}");
            } else {
                println!("lcp: received Protocol-Reject but packet was malformed");
            }
        } else if code == 9 {
            let mut frame = Buffer::new();
            frame.begin_ppp(Self::PROTOCOL);
            frame.write(&[10, id]);
            let pkt_len = (4 + data.len()) as u16;
            frame.write(&pkt_len.to_be_bytes());
            frame.write(data);
            frame.finish_ppp();
            send(frame);
        } else if code == 10 {
            println!("lcp: received Echo-Reply when no Echo-Request was sent");
        } else if code == 11 {
            println!("lcp: received Discard-Reply");
        } else {
            println!(
                "lcp: received {code:02x}:{id:02x}:{} in {s:?}",
                hexdump(data)
            );
        }
    }
    fn judge<'a>(&mut self, option: Self::Opt<'a>, buf: &'a [u8]) -> Verdict {
        use super::lcp::Opt;
        match option {
            Opt::Mru(mru) => {
                if usize::from(mru) == PPP_MRU {
                    Verdict::Ack
                } else {
                    Verdict::Nak(Opt::Mru(PPP_MRU as u16).to_bytes())
                }
            }
            Opt::Accm(accm) => {
                if accm == 0 {
                    Verdict::Ack
                } else {
                    Verdict::Nak(Opt::Mru(PPP_MRU as u16).to_bytes())
                }
            }
            Opt::AuthProtocol(_, _) => Verdict::Rej(buf.to_vec()),
            Opt::QualProtocol(_, _) => Verdict::Rej(buf.to_vec()),
            Opt::Magic(_) => Verdict::Rej(buf.to_vec()),
            Opt::PFCompression => Verdict::Rej(buf.to_vec()),
            Opt::ACFCompression => Verdict::Rej(buf.to_vec()),
        }
    }

    fn get_opts(&mut self) -> Vec<u8> {
        use super::lcp::Opt;
        [Opt::Magic(0).to_bytes(), Opt::Accm(0).to_bytes()]
            .into_iter()
            .flatten()
            .collect()
    }
}
