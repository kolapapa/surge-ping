use std::{convert::TryInto, net::IpAddr};

use log::trace;
use pnet_packet::{
    icmp::{
        self, echo_reply::EchoReplyPacket, echo_request::MutableEchoRequestPacket, IcmpPacket,
        IcmpTypes,
    },
    icmpv6::{Icmpv6Packet, Icmpv6Types, MutableIcmpv6Packet},
    ipv4::Ipv4Packet,
    Packet,
};

use crate::error::{MalformedPacketError, Result, SurgeError};

#[derive(Debug)]
pub struct EchoRequest {
    pub destination: IpAddr,
    pub ident: u16,
    pub seq_cnt: u16,
    pub size: usize,
}

impl EchoRequest {
    pub fn new(destination: IpAddr, ident: u16, seq_cnt: u16, size: usize) -> Self {
        EchoRequest {
            destination,
            ident,
            seq_cnt,
            size,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        match self.destination {
            IpAddr::V4(_) => self.encode_icmp_v4(),
            IpAddr::V6(_) => self.encode_icmp_v6(),
        }
    }

    /// Encodes as an ICMPv4 EchoRequest.
    fn encode_icmp_v4(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0; 8 + self.size]; // 8 bytes of header, then payload
        let mut packet =
            MutableEchoRequestPacket::new(&mut buf[..]).ok_or(SurgeError::IncorrectBufferSize)?;
        packet.set_icmp_type(IcmpTypes::EchoRequest);
        packet.set_identifier(self.ident);
        packet.set_sequence_number(self.seq_cnt);

        // Calculate and set the checksum
        let icmp_packet =
            IcmpPacket::new(packet.packet()).ok_or(SurgeError::IncorrectBufferSize)?;
        let checksum = icmp::checksum(&icmp_packet);
        packet.set_checksum(checksum);

        Ok(packet.packet().to_vec())
    }

    /// Encodes as an ICMPv6 EchoRequest.
    fn encode_icmp_v6(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; 4 + 2 + 2 + self.size]; // 4 bytes ICMP header + 2 bytes ident + 2 bytes sequence, then payload
        let mut packet =
            MutableIcmpv6Packet::new(&mut buf[..]).ok_or(SurgeError::IncorrectBufferSize)?;
        packet.set_icmpv6_type(Icmpv6Types::EchoRequest);

        // Encode the identifier and sequence directly in the payload
        let mut payload = vec![0; 4];
        payload[0..2].copy_from_slice(&self.ident.to_be_bytes()[..]);
        payload[2..4].copy_from_slice(&self.seq_cnt.to_be_bytes()[..]);
        packet.set_payload(&payload);

        // Per https://tools.ietf.org/html/rfc3542#section-3.1 the checksum is
        // omitted, the kernel will insert it.

        Ok(packet.packet().to_vec())
    }
}

/// `EchoReply` struct, which contains some packet information.
#[derive(Debug)]
pub struct EchoReply {
    /// IP Time To Live for outgoing packets. Present for ICMPv4 replies,
    /// absent for ICMPv6 replies.
    pub ttl: Option<u8>,
    /// Source address of ICMP packet.
    pub source: IpAddr,
    /// Sequence of ICMP packet.
    pub sequence: u16,
    /// Identifier of ICMP packet.
    pub identifier: u16,
    /// Size of ICMP packet.
    pub size: usize,
}

impl EchoReply {
    /// Unpack IP packets received from socket as `EchoReply` struct.
    pub fn decode(addr: IpAddr, buf: &[u8]) -> Result<EchoReply> {
        match addr {
            IpAddr::V4(_) => decode_icmpv4(addr, &buf),
            IpAddr::V6(_) => decode_icmpv6(addr, &buf),
        }
    }
}

/// Decodes an ICMPv4 packet received from an IPv4 raw socket
fn decode_icmpv4(addr: IpAddr, buf: &[u8]) -> Result<EchoReply> {
    let ipv4 = Ipv4Packet::new(buf)
        .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIpv4Packet))?;
    let payload = ipv4.payload();
    let icmp_packet = IcmpPacket::new(payload)
        .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpPacket))?;
    let ty = icmp_packet.get_icmp_type();
    if ty != IcmpTypes::EchoReply {
        trace!(
            "type={:?},code={:?},src={},dst={}",
            ty,
            icmp_packet.get_icmp_code(),
            ipv4.get_source(),
            ipv4.get_destination()
        );
        return Err(SurgeError::NotEchoReply(ty));
    }

    let echo_reply_packet = EchoReplyPacket::new(payload).unwrap();
    Ok(EchoReply {
        ttl: Some(ipv4.get_ttl()),
        source: addr,
        sequence: echo_reply_packet.get_sequence_number(),
        identifier: echo_reply_packet.get_identifier(),
        // TODO: When `EchoReplyPacket::packet_size()` can directly use.
        size: echo_reply_packet.packet().len(),
    })
}

/// Decodes an ICMPv6 packet received from an IPv6 raw socket
fn decode_icmpv6(addr: IpAddr, buf: &[u8]) -> Result<EchoReply> {
    // Per https://tools.ietf.org/html/rfc3542#section-3, ICMPv6 raw sockets
    // *do not* provide access to the complete packet, only the payload,
    // so there is no need to extract the payload as there is for ICMPv4
    // packets.
    let icmp_packet = Icmpv6Packet::new(buf)
        .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpv6Packet))?;
    let ty = icmp_packet.get_icmpv6_type();
    if ty != Icmpv6Types::EchoReply {
        trace!(
            "type={:?},code={:?},src={}",
            ty,
            icmp_packet.get_icmpv6_code(),
            addr
        );
        return Err(SurgeError::NotV6EchoReply(ty));
    }

    // pnet_packet doesn't provide a struct for Icmpv6EchoReply, extract
    // the identifier and sequence directly. Payload must be at least 4 bytes
    // for this to work.
    let payload = icmp_packet.payload();
    if payload.len() < 4 {
        return Err(MalformedPacketError::PayloadTooShort {
            got: payload.len(),
            want: 4,
        }
        .into());
    }
    let identifier = u16::from_be_bytes(payload[0..2].try_into().unwrap());
    let sequence = u16::from_be_bytes(payload[2..4].try_into().unwrap());

    Ok(EchoReply {
        ttl: None,
        source: addr,
        sequence,
        identifier,
        size: payload.len() - 4, // Subtract 4 bytes for ident and sequence
    })
}
