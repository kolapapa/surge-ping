use std::convert::TryInto;
use std::net::Ipv6Addr;

use pnet_packet::icmpv6::{self, Icmpv6Code, Icmpv6Type};
use pnet_packet::Packet;
use pnet_packet::{ipv6, PacketSize};

use crate::error::{MalformedPacketError, Result, SurgeError};

pub fn make_icmpv6_echo_packet(ident: u16, seq_cnt: u16, size: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; 4 + 2 + 2 + size]; // 4 bytes ICMP header + 2 bytes ident + 2 bytes sequence, then payload
    let mut packet =
        icmpv6::MutableIcmpv6Packet::new(&mut buf[..]).ok_or(SurgeError::IncorrectBufferSize)?;
    packet.set_icmpv6_type(icmpv6::Icmpv6Types::EchoRequest);

    // Encode the identifier and sequence directly in the payload
    let mut payload = vec![0; 4];
    payload[0..2].copy_from_slice(&ident.to_be_bytes()[..]);
    payload[2..4].copy_from_slice(&seq_cnt.to_be_bytes()[..]);
    packet.set_payload(&payload);

    // Per https://tools.ietf.org/html/rfc3542#section-3.1 the checksum is
    // omitted, the kernel will insert it.

    Ok(packet.packet().to_vec())
}

#[derive(Debug)]
pub struct Icmpv6Packet {
    pub source: Ipv6Addr,
    pub destination: Ipv6Addr,
    pub max_hop_limit: u8,
    pub icmp_type: Icmpv6Type,
    pub icmp_code: Icmpv6Code,
    pub size: usize,
    pub identifier: u16,
    pub sequence: u16,
}

impl Icmpv6Packet {
    fn new(
        source: Ipv6Addr,
        destination: Ipv6Addr,
        max_hop_limit: u8,
        icmp_type: Icmpv6Type,
        icmp_code: Icmpv6Code,
        size: usize,
        identifier: u16,
        sequence: u16,
    ) -> Self {
        Icmpv6Packet {
            source,
            destination,
            max_hop_limit,
            icmp_type,
            icmp_code,
            size,
            identifier,
            sequence,
        }
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        let ipv6_packet = ipv6::Ipv6Packet::new(buf)
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIpv4Packet))?;
        let payload = ipv6_packet.payload();
        let icmpv6_packet = icmpv6::Icmpv6Packet::new(payload)
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpv6Packet))?;
        let icmpv6_payload = icmpv6_packet.payload();
        match icmpv6_packet.get_icmpv6_type() {
            icmpv6::Icmpv6Types::EchoReply => {
                let identifier = u16::from_be_bytes(icmpv6_payload[0..2].try_into().unwrap());
                let sequence = u16::from_be_bytes(icmpv6_payload[2..4].try_into().unwrap());
                Ok(Icmpv6Packet::new(
                    ipv6_packet.get_source(),
                    ipv6_packet.get_destination(),
                    ipv6_packet.get_hop_limit(),
                    icmpv6_packet.get_icmpv6_type(),
                    icmpv6_packet.get_icmpv6_code(),
                    icmpv6_packet.packet_size(),
                    identifier,
                    sequence,
                ))
            }
            _ => {
                // ipv6 header(40) + icmpv6 echo header(4)
                let identifier = u16::from_be_bytes(payload[44..46].try_into().unwrap());
                let sequence = u16::from_be_bytes(payload[46..48].try_into().unwrap());
                Ok(Icmpv6Packet::new(
                    ipv6_packet.get_source(),
                    ipv6_packet.get_destination(),
                    ipv6_packet.get_hop_limit(),
                    icmpv6_packet.get_icmpv6_type(),
                    icmpv6_packet.get_icmpv6_code(),
                    icmpv6_packet.packet_size(),
                    identifier,
                    sequence,
                ))
            }
        }
    }
}
