use std::net::IpAddr;

use log::trace;
use pnet_packet::{
    icmp::{
        self,
        echo_reply::EchoReplyPacket,
        echo_request::{EchoRequestPacket, MutableEchoRequestPacket},
        time_exceeded::TimeExceededPacket,
        IcmpPacket, IcmpType, IcmpTypes,
    },
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
}

/// `IcmpReply` struct, which contains some packet information.
#[derive(Debug)]
pub struct IcmpReply {
    /// Type of ICMP packet.
    pub icmp_type: IcmpType,
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

impl IcmpReply {
    /// Unpack IP packets received from socket as `IcmpReply` struct.
    pub fn decode(buf: &[u8]) -> Result<IcmpReply> {
        let ipv4 = Ipv4Packet::new(buf)
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIpv4Packet))?;
        let payload = ipv4.payload();
        let icmp_packet = IcmpPacket::new(payload)
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpPacket))?;
        match icmp_packet.get_icmp_type() {
            IcmpTypes::EchoReply => {
                let echo_reply_packet = EchoReplyPacket::new(payload).unwrap();
                Ok(IcmpReply {
                    icmp_type: echo_reply_packet.get_icmp_type(),
                    ttl: Some(ipv4.get_ttl()),
                    source: IpAddr::V4(ipv4.get_source()),
                    sequence: echo_reply_packet.get_sequence_number(),
                    identifier: echo_reply_packet.get_identifier(),
                    // TODO: When `EchoReplyPacket::packet_size()` can directly use.
                    size: echo_reply_packet.packet().len(),
                })
            }
            IcmpTypes::TimeExceeded => {
                let time_exceeded_packet = TimeExceededPacket::new(payload).unwrap();
                let payload = time_exceeded_packet.payload();
                // IP Header: 20, ICMP Header: 8
                let req_echo_request_packet = EchoRequestPacket::new(&payload[20..28]).unwrap();
                Ok(IcmpReply {
                    icmp_type: time_exceeded_packet.get_icmp_type(),
                    ttl: Some(ipv4.get_ttl()),
                    source: IpAddr::V4(ipv4.get_source()),
                    sequence: req_echo_request_packet.get_sequence_number(),
                    identifier: req_echo_request_packet.get_identifier(),
                    size: payload.len(),
                })
            }
            other => {
                trace!(
                    "type={:?},code={:?},src={},dst={}",
                    other,
                    icmp_packet.get_icmp_code(),
                    ipv4.get_source(),
                    ipv4.get_destination()
                );
                Err(SurgeError::UnrealizedIcmpType(other))
            }
        }
    }
}
