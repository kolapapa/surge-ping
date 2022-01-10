mod client;
mod config;
mod error;
mod icmp;
mod ping;

pub use client::Client;
pub use config::Config;
pub use error::SurgeError;
pub use icmp::{icmpv4::Icmpv4Packet, IcmpPacket};
pub use ping::Pinger;

#[derive(Debug, Clone, Copy)]
pub enum ICMP {
    V4,
    V6,
}

impl Default for ICMP {
    fn default() -> Self {
        ICMP::V4
    }
}
