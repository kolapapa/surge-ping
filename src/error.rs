#![allow(dead_code)]
use std::io;

use pnet_packet::{icmp::IcmpType, icmpv6::Icmpv6Type};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SurgeError>;

/// An error resulting from a ping option-setting or send/receive operation.
///
#[derive(Error, Debug)]
pub enum SurgeError {
    #[error("buffer size was too small")]
    IncorrectBufferSize,
    #[error("malformed packet: {0}")]
    MalformedPacket(#[from] MalformedPacketError),
    #[error("io error")]
    IOError(#[from] io::Error),
    #[error("expected echoreply, got {0:?}")]
    NotEchoReply(IcmpType),
    #[error("expected echoreply, got {0:?}")]
    NotV6EchoReply(Icmpv6Type),
    #[error("timeout error")]
    Timeout,
    #[error("other icmp message")]
    OtherICMP,
}

#[derive(Error, Debug)]
pub enum MalformedPacketError {
    #[error("expected an Ipv4Packet")]
    NotIpv4Packet,
    #[error("expected an IcmpPacket payload")]
    NotIcmpPacket,
    #[error("expected an Icmpv6Packet")]
    NotIcmpv6Packet,
    #[error("payload too short, got {got}, want {want}")]
    PayloadTooShort { got: usize, want: usize },
}
