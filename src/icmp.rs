use std::net::{IpAddr, Ipv4Addr};

use log::trace;
use packet::builder::Builder;
use packet::Packet;
use packet::{icmp, ip};

use crate::error::{Result, SurgeError};

#[derive(Debug)]
pub struct EchoRequest {
    pub ident: u16,
    pub seq_cnt: u16,
    pub size: usize,
}

impl EchoRequest {
    pub fn new(ident: u16, seq_cnt: u16, size: usize) -> Self {
        EchoRequest {
            ident,
            seq_cnt,
            size,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let payload = vec![0; self.size];
        let echo_request = icmp::Builder::default()
            .echo()?
            .request()?
            .identifier(self.ident)?
            .sequence(self.seq_cnt)?
            .payload(&payload)?
            .build()?;
        Ok(echo_request)
    }
}

#[derive(Debug)]
pub struct EchoReply {
    pub ttl: u8,
    pub source: Ipv4Addr,
    pub sequence: u16,
    pub identifier: u16,
    pub size: usize,
}

impl EchoReply {
    pub fn decode(addr: IpAddr, buf: &[u8]) -> Result<EchoReply> {
        // dont use `ip::v4::Packet::new(buf)?`.
        // Because `buf.as_ref().len() < packet.length() as usize` is always true.
        let ip_packet = ip::v4::Packet::no_payload(buf)?;
        if ip_packet.source() != addr {
            return Err(SurgeError::OtherICMP);
        }
        let packet = icmp::Packet::new(ip_packet.payload())?;
        if packet.kind() == icmp::Kind::EchoReply {
            let echo_reply = packet.echo()?;
            Ok(EchoReply {
                ttl: ip_packet.ttl(),
                source: ip_packet.source(),
                sequence: echo_reply.sequence(),
                identifier: echo_reply.identifier(),
                size: echo_reply.payload().as_ref().len(),
            })
        } else {
            trace!(
                "type={:?},code={},src={},dst={}",
                packet.kind(),
                packet.code(),
                ip_packet.source(),
                ip_packet.destination()
            );
            Err(SurgeError::KindError(packet.kind()))
        }
    }
}
