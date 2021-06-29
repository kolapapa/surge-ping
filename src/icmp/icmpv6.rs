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

/// Packet structure returned by ICMPv6.
#[derive(Debug)]
pub struct Icmpv6Packet {
    source: Ipv6Addr,
    destination: Ipv6Addr,
    max_hop_limit: u8,
    icmpv6_type: Icmpv6Type,
    icmpv6_code: Icmpv6Code,
    size: usize,
    identifier: u16,
    sequence: u16,
}

impl Default for Icmpv6Packet {
    fn default() -> Self {
        Icmpv6Packet {
            source: Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1),
            destination: Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1),
            max_hop_limit: 0,
            icmpv6_type: Icmpv6Type::new(0),
            icmpv6_code: Icmpv6Code::new(0),
            size: 0,
            identifier: 0,
            sequence: 0,
        }
    }
}

impl Icmpv6Packet {
    fn source(&mut self, source: Ipv6Addr) -> &mut Self {
        self.source = source;
        self
    }

    /// Get the source IPv6 address.
    pub fn get_source(&self) -> Ipv6Addr {
        self.source
    }

    fn destination(&mut self, destination: Ipv6Addr) -> &mut Self {
        self.destination = destination;
        self
    }

    /// Get the destination IPv6 address.
    pub fn get_destination(&self) -> Ipv6Addr {
        self.destination
    }

    fn max_hop_limit(&mut self, max_hop_limit: u8) -> &mut Self {
        self.max_hop_limit = max_hop_limit;
        self
    }

    /// Get the hop_limit field.
    pub fn get_max_hop_limit(&self) -> u8 {
        self.max_hop_limit
    }

    fn icmpv6_type(&mut self, icmpv6_type: Icmpv6Type) -> &mut Self {
        self.icmpv6_type = icmpv6_type;
        self
    }

    /// Get the icmpv6_type of the icmpv6 packet.
    pub fn get_icmpv6_type(&self) -> Icmpv6Type {
        self.icmpv6_type
    }

    fn icmpv6_code(&mut self, icmpv6_code: Icmpv6Code) -> &mut Self {
        self.icmpv6_code = icmpv6_code;
        self
    }

    /// Get the icmpv6_code of the icmpv6 packet.
    pub fn get_icmpv6_code(&self) -> Icmpv6Code {
        self.icmpv6_code
    }

    fn size(&mut self, size: usize) -> &mut Self {
        self.size = size;
        self
    }

    /// Get the size of the icmp_v6 packet.
    pub fn get_size(&self) -> usize {
        self.size
    }

    fn identifier(&mut self, identifier: u16) -> &mut Self {
        self.identifier = identifier;
        self
    }

    /// Get the identifier of the icmp_v6 packet.
    pub fn get_identifier(&self) -> u16 {
        self.identifier
    }

    fn sequence(&mut self, sequence: u16) -> &mut Self {
        self.sequence = sequence;
        self
    }

    /// Get the sequence of the icmp_v6 packet.
    pub fn get_sequence(&self) -> u16 {
        self.sequence
    }

    /// Decode into icmpv6 packet from the socket message.
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
                let mut packet = Icmpv6Packet::default();
                packet
                    .source(ipv6_packet.get_source())
                    .destination(ipv6_packet.get_destination())
                    .max_hop_limit(ipv6_packet.get_hop_limit())
                    .icmpv6_type(icmpv6_packet.get_icmpv6_type())
                    .icmpv6_code(icmpv6_packet.get_icmpv6_code())
                    .size(icmpv6_packet.packet_size())
                    .identifier(identifier)
                    .sequence(sequence);
                Ok(packet)
            }
            _ => {
                // ipv6 header(40) + icmpv6 echo header(4)
                let identifier = u16::from_be_bytes(payload[44..46].try_into().unwrap());
                let sequence = u16::from_be_bytes(payload[46..48].try_into().unwrap());
                let mut packet = Icmpv6Packet::default();
                packet
                    .source(ipv6_packet.get_source())
                    .destination(ipv6_packet.get_destination())
                    .max_hop_limit(ipv6_packet.get_hop_limit())
                    .icmpv6_type(icmpv6_packet.get_icmpv6_type())
                    .icmpv6_code(icmpv6_packet.get_icmpv6_code())
                    .size(icmpv6_packet.packet_size())
                    .identifier(identifier)
                    .sequence(sequence);
                Ok(packet)
            }
        }
    }
}
