use std::net::Ipv4Addr;

use packet::builder::Builder;
use packet::Error;
use packet::Packet;
use packet::{icmp, ip};
use rand::random;

const TOKEN_SIZE: usize = 8;
pub type Token = [u8; TOKEN_SIZE];

pub fn make_echo_request(ident: u16, seq_cnt: u16, size: usize) -> Result<(Vec<u8>, Token), Error> {
    let token: Token = random();
    let mut payload = vec![0; size];
    {
        let (left, _) = payload.split_at_mut(TOKEN_SIZE);
        left.copy_from_slice(&token);
    }

    let echo_request = icmp::Builder::default()
        .echo()?
        .request()?
        .identifier(ident)?
        .sequence(seq_cnt)?
        .payload(&payload)?
        .build()?;
    Ok((echo_request, token))
}

#[derive(Debug)]
pub struct EchoReply {
    pub ttl: u8,
    pub source: Ipv4Addr,
    pub sequence: u16,
    pub size: usize,
    pub token: Token,
}

impl EchoReply {
    pub fn decode(buf: &[u8]) -> Result<Self, Error> {
        // dont use `ip::v4::Packet::new(buf)?`.
        // Because `buf.as_ref().len() < packet.length() as usize` is always true.
        let ip_packet = ip::v4::Packet::no_payload(buf)?;
        let packet = icmp::Packet::new(ip_packet.payload())?;
        let echo_reply = packet.echo()?;
        if !echo_reply.is_reply() {
            return Err(Error::InvalidPacket);
        }
        if echo_reply.payload().as_ref().len() < TOKEN_SIZE {
            return Err(Error::InvalidPacket);
        }
        let mut token: Token = [0; TOKEN_SIZE];
        token.copy_from_slice(&echo_reply.payload()[..TOKEN_SIZE]);

        Ok(EchoReply {
            ttl: ip_packet.ttl(),
            source: ip_packet.source(),
            sequence: echo_reply.sequence(),
            size: echo_reply.payload().as_ref().len(),
            token,
        })
    }
}
