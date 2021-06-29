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

/// Packet structure returned by ICMPv4.
#[derive(Debug)]
pub struct Icmpv4Packet {
    source: Ipv4Addr,
    destination: Ipv4Addr,
    ttl: u8,
    icmp_type: IcmpType,
    icmp_code: IcmpCode,
    size: usize,
    real_dest: Ipv4Addr,
    identifier: u16,
    sequence: u16,
}

impl Default for Icmpv4Packet {
    fn default() -> Self {
        Icmpv4Packet {
            source: Ipv4Addr::new(127, 0, 0, 1),
            destination: Ipv4Addr::new(127, 0, 0, 1),
            ttl: 0,
            icmp_type: IcmpType::new(0),
            icmp_code: IcmpCode::new(0),
            size: 0,
            real_dest: Ipv4Addr::new(127, 0, 0, 1),
            identifier: 0,
            sequence: 0,
        }
    }
}

impl Icmpv4Packet {
    fn source(&mut self, source: Ipv4Addr) -> &mut Self {
        self.source = source;
        self
    }

    /// Get the source field.
    pub fn get_source(&self) -> Ipv4Addr {
        self.source
    }

    fn destination(&mut self, destination: Ipv4Addr) -> &mut Self {
        self.destination = destination;
        self
    }

    /// Get the destination field.
    pub fn get_destination(&self) -> Ipv4Addr {
        self.destination
    }

    fn ttl(&mut self, ttl: u8) -> &mut Self {
        self.ttl = ttl;
        self
    }

    /// Get the ttl field.
    pub fn get_ttl(&self) -> u8 {
        self.ttl
    }

    fn icmp_type(&mut self, icmp_type: IcmpType) -> &mut Self {
        self.icmp_type = icmp_type;
        self
    }

    /// Get the icmp_type of the icmpv4 packet.
    pub fn get_icmp_type(&self) -> IcmpType {
        self.icmp_type
    }

    fn icmp_code(&mut self, icmp_code: IcmpCode) -> &mut Self {
        self.icmp_code = icmp_code;
        self
    }

    /// Get the icmp_code of the icmpv4 packet.
    pub fn get_icmp_code(&self) -> IcmpCode {
        self.icmp_code
    }

    fn size(&mut self, size: usize) -> &mut Self {
        self.size = size;
        self
    }

    /// Get the size of the icmp_v4 packet.
    pub fn get_size(&self) -> usize {
        self.size
    }

    fn real_dest(&mut self, addr: Ipv4Addr) -> &mut Self {
        self.real_dest = addr;
        self
    }

    /// If it is an `echo_reply` packet, it is the source address in the IPv4 packet.
    /// If it is other packets, it is the destination address in the IPv4 packet in ICMP's payload.
    pub fn get_real_dest(&self) -> Ipv4Addr {
        self.real_dest
    }

    fn identifier(&mut self, identifier: u16) -> &mut Self {
        self.identifier = identifier;
        self
    }

    /// Get the identifier of the icmp_v4 packet.
    pub fn get_identifier(&self) -> u16 {
        self.identifier
    }

    fn sequence(&mut self, sequence: u16) -> &mut Self {
        self.sequence = sequence;
        self
    }

    /// Get the sequence of the icmp_v4 packet.
    pub fn get_sequence(&self) -> u16 {
        self.sequence
    }

    /// Decode into icmp packet from the socket message.
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
                let mut packet = Icmpv4Packet::default();
                packet
                    .source(ipv4_packet.get_source())
                    .destination(ipv4_packet.get_destination())
                    .ttl(ipv4_packet.get_ttl())
                    .icmp_type(icmp_packet.get_icmp_type())
                    .icmp_code(icmp_packet.get_icmp_code())
                    .size(icmp_packet.packet().len())
                    .real_dest(ipv4_packet.get_source())
                    .identifier(icmp_packet.get_identifier())
                    .sequence(icmp_packet.get_sequence_number());
                Ok(packet)
            }
            _ => {
                let icmp_payload = icmp_packet.payload();
                // icmp unused(4) + ip header(20) + echo icmp(4)
                let real_ip_packet = ipv4::Ipv4Packet::new(&icmp_payload[4..])
                    .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIpv4Packet))?;
                let identifier = u16::from_be_bytes(icmp_payload[28..30].try_into().unwrap());
                let sequence = u16::from_be_bytes(icmp_payload[30..32].try_into().unwrap());
                let mut packet = Icmpv4Packet::default();
                packet
                    .source(ipv4_packet.get_source())
                    .destination(ipv4_packet.get_destination())
                    .ttl(ipv4_packet.get_ttl())
                    .icmp_type(icmp_packet.get_icmp_type())
                    .icmp_code(icmp_packet.get_icmp_code())
                    .size(icmp_packet.packet_size())
                    .real_dest(real_ip_packet.get_destination())
                    .identifier(identifier)
                    .sequence(sequence);
                Ok(packet)
            }
        }
    }
}
