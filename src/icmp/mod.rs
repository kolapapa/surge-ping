use std::net::IpAddr;

pub mod icmpv4;
pub mod icmpv6;

/// Represents the ICMP reply packet.
#[derive(Debug)]
pub enum IcmpPacket {
    /// An ICMPv4 packet abstraction.
    V4(icmpv4::Icmpv4Packet),
    /// An ICMPv6 packet abstraction.
    V6(icmpv6::Icmpv6Packet),
}

impl IcmpPacket {
    /// Check reply Icmp packet is corret.
    pub fn check_reply_packet(&self, destination: IpAddr, seq_cnt: u16, identifier: u16) -> bool {
        match self {
            IcmpPacket::V4(packet) => {
                destination.eq(&IpAddr::V4(packet.get_source()))
                    && packet.get_sequence() == seq_cnt
                    && packet.get_identifier() == identifier
            }
            IcmpPacket::V6(packet) => {
                destination.eq(&IpAddr::V6(packet.get_source()))
                    && packet.get_sequence() == seq_cnt
                    && packet.get_identifier() == identifier
            }
        }
    }
}
