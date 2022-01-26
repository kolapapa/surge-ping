mod client;
mod config;
mod error;
mod icmp;
mod ping;

use std::net::IpAddr;

pub use client::Client;
pub use config::Config;
pub use error::SurgeError;
pub use icmp::{icmpv4::Icmpv4Packet, icmpv6::Icmpv6Packet, IcmpPacket};
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

/// Shortcut method to quickly make a `Pinger`.
/// **NOTE**: This function creates a new internal `Client` on each call,
/// and so should not be used if making many target. Create a
/// [`Client`](./struct.Client.html) instead.
///
/// # Examples
///
/// ```rust
/// let mut pinger = surge_ping::pinger("8.8.8.8".parse()?).await?;
/// ```
///
/// # Errors
///
/// This function fails if:
///
/// - socket create failed
///
pub async fn pinger(host: IpAddr) -> Result<Pinger, SurgeError> {
    let config = match host {
        IpAddr::V4(_) => Config::default(),
        IpAddr::V6(_) => Config::builder().kind(ICMP::V6).build(),
    };
    let client = Client::new(&config)?;
    let pinger = client.pinger(host).await;
    Ok(pinger)
}
