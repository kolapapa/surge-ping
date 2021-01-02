use std::net::Ipv4Addr;

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
    pub size: usize,
}

impl EchoReply {
    pub fn decode(buf: &[u8]) -> Result<Self> {
        // dont use `ip::v4::Packet::new(buf)?`.
        // Because `buf.as_ref().len() < packet.length() as usize` is always true.
        let ip_packet = ip::v4::Packet::no_payload(buf)?;
        let packet = icmp::Packet::new(ip_packet.payload())?;
        let echo_reply = packet.echo()?;
        if !echo_reply.is_reply() {
            return Err(SurgeError::KindError);
        }

        Ok(EchoReply {
            ttl: ip_packet.ttl(),
            source: ip_packet.source(),
            sequence: echo_reply.sequence(),
            size: echo_reply.payload().as_ref().len(),
        })
    }
}
