use crate::arch::time::now;
use crate::net::control::{ControlAutomaton, Ipcp, Lcp, Packet};
use crate::net::hdlc::{
    HDLC_ADDRESS, HDLC_ADDRESS_LEN, HDLC_CONTROL, HDLC_CONTROL_LEN, HDLC_ESC, HDLC_FCS_LEN,
    HDLC_FLAG, HDLC_FLAG_LEN, ppp_calc_fcs,
};
use crate::net::phy::{CHANNEL2_RX, CHANNEL2_TX, send};
use crate::{println, smoltcp_now};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec;
use core::cell::RefCell;
use core::time::Duration;
use d1_pac::Peripherals;
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::phy::{DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use smoltcp::wire::{IpAddress, IpCidr};

pub mod control;
pub mod hdlc;
pub mod ipcp;
pub mod lcp;
pub mod phy;

const PPP_PROTOCOL_BYTES: usize = 2;

/// Maximum PPP frame size (not including protocol or HDLC framing).
const PPP_MRU: usize = 1500;

/// PPP Protocol identifier for LCP.
const PROTO_LCP: u16 = 0xc021;
const PROTO_IPCP: u16 = 0x8021;
const PROTO_IP: u16 = 0x0021;

/// Maximum frame size for PPP in HDLC-like framing. We require a fixed MRU [`PPP_MRU`] so that we
/// can use fixed-size buffers.
const MAX_FRAME: usize = HDLC_FLAG_LEN
    + HDLC_ADDRESS_LEN
    + HDLC_CONTROL_LEN
    + PPP_PROTOCOL_BYTES
    + PPP_MRU
    + HDLC_FCS_LEN;

#[allow(dead_code)]
static ALLOWED_PROTOCOLS: &[u16] = &[PROTO_LCP];

/// PPP Frame
#[derive(Debug)]
pub struct Buffer {
    len: usize,
    bytes: [u8; MAX_FRAME],
}
// Core impl
impl Buffer {
    const fn new() -> Self {
        Self {
            len: 0,
            bytes: [0; MAX_FRAME],
        }
    }
}
// Utilities for constructing PPP frames in Buffers.
impl Buffer {
    pub fn begin_ppp(&mut self, proto: u16) {
        self.bytes[0] = HDLC_FLAG;
        self.bytes[1] = HDLC_ADDRESS;
        self.bytes[2] = HDLC_CONTROL;
        self.bytes[3..5].copy_from_slice(&proto.to_be_bytes());
        self.len = 5;
    }
    pub fn write(&mut self, b: &[u8]) {
        self.bytes[self.len..self.len + b.len()].copy_from_slice(b);
        self.len += b.len();
    }
    pub fn finish_ppp(&mut self) {
        let fcs = ppp_calc_fcs(&self.bytes[1..self.len]);
        self.bytes[self.len..self.len + 2].copy_from_slice(&fcs.to_le_bytes());
        self.bytes[self.len + 2] = HDLC_FLAG;
        self.len += 3;
    }
}

struct PppInner {
    lcp: ControlAutomaton<Lcp>,
    ipcp: ControlAutomaton<Ipcp>,
    ip_recv_queue: VecDeque<Box<[u8]>>,
    ip_xmit_queue: VecDeque<Box<[u8]>>,
}
impl PppInner {
    fn new() -> Self {
        Self {
            lcp: ControlAutomaton::new(Lcp::new()),
            ipcp: ControlAutomaton::new(Ipcp::new()),
            ip_recv_queue: Default::default(),
            ip_xmit_queue: Default::default(),
        }
    }

    // -- section: control loop
    fn poll(&mut self) {
        if self.lcp.is_closed() {
            self.lcp.open()
        }
        if !self.lcp.is_layer_up() && !self.ipcp.is_closed() {
            self.ipcp.close();
        }
        if self.lcp.is_layer_up() && self.ipcp.is_closed() {
            self.ipcp.open();
        }
        if self.ipcp.is_layer_up() {
            while let Some(hd) = self.ip_xmit_queue.pop_front() {
                let mut buffer = Buffer::new();
                buffer.begin_ppp(PROTO_IP);
                buffer.write(&hd);
                buffer.finish_ppp();
                send(buffer);
            }
        }
    }
    fn recv_lcp(&mut self, packet: &[u8]) {
        self.lcp.received(packet);
    }
    fn recv_ipcp(&mut self, packet: &[u8]) {
        self.ipcp.received(packet);
    }
    fn recv_ip(&mut self, packet: &[u8]) {
        self.ip_recv_queue.push_back(Box::from(packet));
    }

    // -- section: phy::Device backend
    fn send_ip(&mut self, packet: &[u8]) {
        self.ip_xmit_queue.push_back(Box::from(packet));
    }
    fn poll_ip(&mut self) -> Option<Box<[u8]>> {
        self.ip_recv_queue.pop_front()
    }
}
struct PppPhy {
    inner: Rc<RefCell<PppInner>>,
}
impl PppPhy {
    pub fn new(inner: Rc<RefCell<PppInner>>) -> Self {
        Self { inner }
    }
}
impl smoltcp::phy::Device for PppPhy {
    #[rustfmt::skip]
    type RxToken<'a> = PppPhyRxToken where Self: 'a;
    #[rustfmt::skip]
    type TxToken<'a> = PppPhyTxToken where Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        self.inner
            .borrow_mut()
            .poll_ip()
            .map(|buf| (PppPhyRxToken(buf), PppPhyTxToken(self.inner.clone())))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(PppPhyTxToken(self.inner.clone()))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = PPP_MRU;
        caps.max_burst_size = Some(16);
        caps.medium = Medium::Ip;
        caps
    }
}
struct PppPhyRxToken(Box<[u8]>);
impl smoltcp::phy::RxToken for PppPhyRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.0)
    }
}
struct PppPhyTxToken(Rc<RefCell<PppInner>>);
impl smoltcp::phy::TxToken for PppPhyTxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0; len];
        let res = f(&mut buf);
        self.0.borrow_mut().send_ip(&buf);
        res
    }
}

// Primary loop for the network system
pub fn run() {
    link_init();

    let ppp_backend = Rc::new(RefCell::new(PppInner::new()));
    let mut ppp = PppPhy::new(ppp_backend.clone());
    let mut iface = Interface::new(
        smoltcp::iface::Config::new(smoltcp::wire::HardwareAddress::Ip),
        &mut ppp,
        smoltcp_now(),
    );

    let mut lcp_rej_id = 0;

    use smoltcp::socket::tcp;
    // let udp_rxb = udp::PacketBuffer::new(
    //     vec![udp::PacketMetadata::EMPTY, udp::PacketMetadata::EMPTY],
    //     vec![0; 65535],
    // );
    // let udp_txb = udp::PacketBuffer::new(
    //     vec![udp::PacketMetadata::EMPTY, udp::PacketMetadata::EMPTY],
    //     vec![0; 65535],
    // );
    // let udp_socket = udp::Socket::new(udp_rxb, udp_txb);

    let tcp_rx_buffer = tcp::SocketBuffer::new(vec![0; 65535]);
    let tcp_tx_buffer = tcp::SocketBuffer::new(vec![0; 65535]);
    let mut tcp_socket = tcp::Socket::new(tcp_rx_buffer, tcp_tx_buffer);
    tcp_socket.set_keep_alive(Some(smoltcp::time::Duration::from_secs(1)));

    let mut active = false;
    let mut last_send = now();

    let mut sockets = SocketSet::new(vec![]);
    // let udp_handle = sockets.add(udp_socket);
    let tcp_handle = sockets.add(tcp_socket);

    loop {
        let Some((buf_len, buf)) = critical_section::with(|cs| {
            let channel = unsafe { CHANNEL2_RX.borrow(cs).get().as_mut_unchecked() };
            channel.read_head_buf().map(|buf| (buf.len, &buf.bytes))
        }) else {
            let (ip_update, ip_up) = {
                let mut bak = ppp_backend.borrow_mut();
                bak.poll();
                let ip_up = bak.ipcp.is_layer_up();
                let ip_update = if ip_up {
                    bak.ipcp.layer_mut().get_host_addr_update()
                } else {
                    None
                };
                (ip_update, ip_up)
            };
            if let Some((old, new)) = ip_update {
                iface.update_ip_addrs(|ip_addrs| {
                    // always /32 for us
                    let new_cidr = IpCidr::new(IpAddress::Ipv4(new), 32);
                    if let Some(old) = old {
                        let prev_cidr = IpCidr::new(IpAddress::Ipv4(old), 32);
                        let i = ip_addrs.iter().position(|x| *x == prev_cidr).unwrap();
                        ip_addrs[i] = new_cidr;
                    } else {
                        ip_addrs.push(new_cidr).unwrap();
                    }
                });
            }
            if ip_up {
                iface.poll(smoltcp_now(), &mut ppp, &mut sockets);

                let socket = sockets.get_mut::<tcp::Socket>(tcp_handle);
                if !socket.is_open() {
                    socket.listen(8000).unwrap();
                }
                if socket.is_active() && !active {
                    println!("tcp:8000 connected");
                    last_send = now();
                } else if !socket.is_active() && active {
                    println!("tcp:8000 disconnected");
                }
                active = socket.is_active();

                if socket.may_recv() {
                    let data = socket
                        .recv(|buffer| {
                            let len = buffer.len();
                            (len, buffer.to_owned())
                        })
                        .unwrap();
                    let response;
                    if !data.is_empty() {
                        println!("\x1b[32mtcp\x1b[0m: < {}", String::from_utf8_lossy(&data));
                        let mut headers = vec![httparse::EMPTY_HEADER; 64];
                        let mut req = httparse::Request::new(&mut headers);
                        req.parse(&data).unwrap();
                        println!("\x1b[32mhttp\x1b[0m: {req:?}");
                        let peri = unsafe { Peripherals::steal() };
                        match req.path {
                            None => {
                                response = Some("HTTP/1.0 400 BAD REQUEST\r\n\r\n\r\n");
                            }
                            Some("/on") => {
                                peri.GPIO
                                    .pd_dat()
                                    .modify(|r, w| w.pd_dat().variant(r.pd_dat().bits() | 1));
                                response = Some("HTTP/1.0 200 OK\r\n\r\n\r\n");
                            }
                            Some("/off") => {
                                peri.GPIO
                                    .pd_dat()
                                    .modify(|r, w| w.pd_dat().variant(r.pd_dat().bits() & !1));
                                response = Some("HTTP/1.0 200 OK\r\n\r\n\r\n");
                            }
                            Some(_) => {
                                response = Some("HTTP/1.0 404 NOT FOUND\r\n\r\n\r\n");
                            }
                        }
                    } else {
                        response = None;
                    }
                    if socket.can_send() && !data.is_empty() {
                        last_send = now();
                        socket.send_slice(response.unwrap().as_bytes()).unwrap();
                        println!("\x1b[32mtcp\x1b[0m: > {}", response.unwrap());
                        socket.close();
                    }
                } else if socket.may_send() {
                    socket.close();
                }
                if socket.is_active() {
                    if (now() - last_send) > Duration::from_secs(10) {
                        println!("\x1b[31mtcp: abort\x1b[0m");
                        socket.abort();
                    }
                }

                // let socket = sockets.get_mut::<udp::Socket>(udp_handle);
                // if !socket.is_open() {
                //     println!("udp: bind to port 67");
                //     socket.bind(67).unwrap();
                // } else {
                //     match socket.recv() {
                //         Ok((packet, metadata)) => {
                //             println!("udp: packet {packet:?} metadata={metadata}");
                //         }
                //         Err(RecvError::Exhausted) => {}
                //         Err(e) => {
                //             println!("udp: recv error: {e}");
                //         }
                //     }
                // }

                // let event = sockets.get_mut::<dhcp::Socket>(dhcp_handle).poll();
                // match event {
                //     None => {}
                //     Some(dhcp::Event::Configured(config)) => {
                //         debug!("dhcp: config acquired");
                //         {
                //             ppp_backend
                //                 .borrow_mut()
                //                 .ipcp
                //                 .layer_mut()
                //                 .set_host_ip(config.address.address());
                //             // force renegotiation
                //             ppp_backend.borrow_mut().ipcp.close();
                //         }
                //         if let Some(router) = config.router {
                //             debug!("dhcp: default gateway: {}", router);
                //             iface.routes_mut().add_default_ipv4_route(router).unwrap();
                //         } else {
                //             debug!("dhcp: no default gateway");
                //             iface.routes_mut().remove_default_ipv4_route();
                //         }
                //         for (i, s) in config.dns_servers.iter().enumerate() {
                //             debug!("dhcp: dns server {i}:\t{s}");
                //         }
                //     }
                //     Some(dhcp::Event::Deconfigured) => {
                //         debug!("dhcp: lost config");
                //         iface.update_ip_addrs(|addrs| addrs.clear());
                //         iface.routes_mut().remove_default_ipv4_route();
                //     }
                // }
            }
            continue;
        };
        let hdlc_frame = &buf[..buf_len];
        let Ok(ppp_frame) = try_from_hdlc_frame(hdlc_frame) else {
            discard();
            continue;
        };
        let (protocol_bytes, packet) = ppp_frame.split_at(2);
        // PANIC SAFETY: try_from_hdlc_frame asserts that hdlc-frame is at least 7 bytes, which
        // means that the encapsulated PPP frame must be at least 2 bytes.
        let protocol = u16::from_be_bytes(protocol_bytes.try_into().unwrap());
        match protocol {
            PROTO_LCP => ppp_backend.borrow_mut().recv_lcp(packet),
            PROTO_IPCP if ppp.inner.borrow().lcp.is_layer_up() => {
                let did_open = {
                    let was_open = ppp_backend.borrow().ipcp.is_layer_up();
                    ppp_backend.borrow_mut().recv_ipcp(packet);
                    ppp_backend.borrow().ipcp.is_layer_up() && !was_open
                };
                if did_open {
                    // let peer = { ppp_backend.borrow().ipcp.layer().peer_addr().unwrap() };
                    // iface.routes_mut().add_default_ipv4_route(peer).unwrap();
                }
            }
            PROTO_IP if ppp.inner.borrow().ipcp.is_layer_up() => {
                ppp.inner.borrow_mut().recv_ip(packet)
            }
            _ => {
                // unrecognized
                println!("ppp: unrecognized protocol: {:04x}", protocol);
                let bak = ppp_backend.borrow_mut();
                if bak.lcp.is_layer_up() {
                    let mut buf = Buffer::new();
                    buf.begin_ppp(PROTO_LCP);
                    buf.write(&[8, lcp_rej_id]);
                    lcp_rej_id += 1;
                    let pkt_len = 6 + packet.len() as u16;
                    buf.write(&pkt_len.to_be_bytes());
                    buf.write(&protocol.to_be_bytes());
                    buf.write(packet);
                    buf.finish_ppp();
                    send(buf);
                }
            }
        }

        discard();
    }
}

/// Try to extract a PPP frame from an HDLC frame.
fn try_from_hdlc_frame(hdlc_frame: &[u8]) -> Result<&[u8], ()> {
    if hdlc_frame.len() < 7 {
        println!("ppp: frame too short: {}", hexdump(&hdlc_frame));
        Err(())
    } else if hdlc_frame[0] != HDLC_FLAG
        || hdlc_frame[1] != HDLC_ADDRESS
        || hdlc_frame[2] != HDLC_CONTROL
    {
        println!("ppp: not HDLC leader: {}", hexdump(&hdlc_frame[..3]));
        Err(())
    } else {
        let fcs_field = u16::from_le_bytes(hdlc_frame[hdlc_frame.len() - 2..].try_into().unwrap());
        // not totally sure this is what we're supposed to do...
        let fcs_calc = ppp_calc_fcs(&hdlc_frame[1..hdlc_frame.len() - 2]);
        if fcs_calc != fcs_field {
            println!("ppp: bad FCS: stated fcs={fcs_field:04x}, calculated fcs={fcs_calc:04x}");
            Err(())
        } else {
            Ok(&hdlc_frame[3..hdlc_frame.len() - 2])
        }
    }
}

/// Initialize the link with the peer asyncmap
pub fn link_init() {
    critical_section::with(|cs| {
        let xmit = unsafe { CHANNEL2_TX.borrow(cs).get().as_mut_unchecked() };
        xmit.needs_escape(HDLC_ESC);
        xmit.needs_escape(HDLC_FLAG);
    });
}
/// Discard the current head of the receive buffer
pub fn discard() {
    critical_section::with(|cs| unsafe {
        CHANNEL2_RX
            .borrow(cs)
            .get()
            .as_mut_unchecked()
            .free_buffer()
    });
}

pub fn hexdump(buf: &[u8]) -> String {
    buf.iter()
        .map(|x| {
            let a = ((x & 0xf0) >> 4) as u32;
            let b = (x & 0x0f) as u32;
            [
                char::from_digit(a, 16).unwrap(),
                char::from_digit(b, 16).unwrap(),
            ]
        })
        .intersperse([' ', '\t'])
        .flatten()
        .filter(|&x| x != '\t')
        .collect::<String>()
}
