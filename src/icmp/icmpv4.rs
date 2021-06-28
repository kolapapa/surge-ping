use std::convert::TryInto;
use std::net::Ipv4Addr;

use pnet_packet::icmp::{self, IcmpCode, IcmpType};
use pnet_packet::Packet;
use pnet_packet::{ipv4, PacketSize};

use crate::error::{MalformedPacketError, Result, SurgeError};

pub fn make_icmpv4_echo_packet(ident: u16, seq_cnt: u16, size: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0; 8 + size]; // 8 bytes of header, then payload
    let mut packet = icmp::echo_request::MutableEchoRequestPacket::new(&mut buf[..])
        .ok_or(SurgeError::IncorrectBufferSize)?;
    packet.set_icmp_type(icmp::IcmpTypes::EchoRequest);
    packet.set_identifier(ident);
    packet.set_sequence_number(seq_cnt);

    // Calculate and set the checksum
    let icmp_packet =
        icmp::IcmpPacket::new(packet.packet()).ok_or(SurgeError::IncorrectBufferSize)?;
    let checksum = icmp::checksum(&icmp_packet);
    packet.set_checksum(checksum);

    Ok(packet.packet().to_vec())
}

#[derive(Debug)]
pub struct Icmpv4Packet {
    pub source: Ipv4Addr,
    pub destination: Ipv4Addr,
    pub ttl: u8,
    pub icmp_type: IcmpType,
    pub icmp_code: IcmpCode,
    pub size: usize,
    pub identifier: u16,
    pub sequence: u16,
}

impl Icmpv4Packet {
    fn new(
        source: Ipv4Addr,
        destination: Ipv4Addr,
        ttl: u8,
        icmp_type: IcmpType,
        icmp_code: IcmpCode,
        size: usize,
        identifier: u16,
        sequence: u16,
    ) -> Self {
        Icmpv4Packet {
            source,
            destination,
            ttl,
            icmp_type,
            icmp_code,
            size,
            identifier,
            sequence,
        }
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        let ipv4_packet = ipv4::Ipv4Packet::new(buf)
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIpv4Packet))?;
        let payload = ipv4_packet.payload();
        let icmp_packet = icmp::IcmpPacket::new(payload)
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpv4Packet))?;
        match icmp_packet.get_icmp_type() {
            icmp::IcmpTypes::EchoReply => {
                let icmp_packet = icmp::echo_reply::EchoReplyPacket::new(payload)
                    .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpv4Packet))?;
                Ok(Icmpv4Packet::new(
                    ipv4_packet.get_source(),
                    ipv4_packet.get_destination(),
                    ipv4_packet.get_ttl(),
                    icmp_packet.get_icmp_type(),
                    icmp_packet.get_icmp_code(),
                    icmp_packet.packet().len(),
                    icmp_packet.get_identifier(),
                    icmp_packet.get_sequence_number(),
                ))
            }
            _ => {
                let icmp_payload = icmp_packet.payload();
                // ip header(20) + echo icmp(4)
                let identifier = u16::from_be_bytes(icmp_payload[24..26].try_into().unwrap());
                let sequence = u16::from_be_bytes(icmp_payload[26..28].try_into().unwrap());
                Ok(Icmpv4Packet::new(
                    ipv4_packet.get_source(),
                    ipv4_packet.get_destination(),
                    ipv4_packet.get_ttl(),
                    icmp_packet.get_icmp_type(),
                    icmp_packet.get_icmp_code(),
                    icmp_packet.packet_size(),
                    identifier,
                    sequence,
                ))
            }
        }
    }
}
