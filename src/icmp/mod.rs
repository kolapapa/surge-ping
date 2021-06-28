use std::net::IpAddr;

pub mod icmpv4;
pub mod icmpv6;

#[derive(Debug)]
pub enum IcmpPacket {
    V4(icmpv4::Icmpv4Packet),
    V6(icmpv6::Icmpv6Packet),
}

impl IcmpPacket {
    pub fn check_reply_packet(&self, destination: IpAddr, seq_cnt: u16, identifier: u16) -> bool {
        match self {
            IcmpPacket::V4(packet) => {
                destination.eq(&IpAddr::V4(packet.source))
                    && packet.sequence == seq_cnt
                    && packet.identifier == identifier
            }
            IcmpPacket::V6(packet) => {
                destination.eq(&IpAddr::V6(packet.source))
                    && packet.sequence == seq_cnt
                    && packet.identifier == identifier
            }
        }
    }
}
