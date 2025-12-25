#![allow(dead_code)]
use std::{io, net::IpAddr};

use thiserror::Error;

use crate::{icmp::PingSequence, PingIdentifier};

pub type Result<T> = std::result::Result<T, SurgeError>;

/// An error resulting from a ping option-setting or send/receive operation.
///
#[derive(Error, Debug)]
pub enum SurgeError {
    #[error("buffer size was too small")]
    IncorrectBufferSize,
    #[error("malformed packet: {0}")]
    MalformedPacket(#[from] MalformedPacketError),
    #[error("io error: {0}")]
    IOError(#[from] io::Error),
    #[error("Request timeout for icmp_seq {seq}")]
    Timeout { seq: PingSequence },
    #[error("Echo Request packet.")]
    EchoRequestPacket,
    #[error("Network error.")]
    NetworkError,
    #[error("Multiple identical request")]
    IdenticalRequests {
        host: IpAddr,
        ident: Option<PingIdentifier>,
        seq: PingSequence,
    },
    #[error("Client has been destroyed, ping operations are no longer available")]
    ClientDestroyed,
}

#[derive(Error, Debug)]
pub enum MalformedPacketError {
    #[error("expected an Ipv4Packet")]
    NotIpv4Packet,
    #[error("expected an Ipv6Packet")]
    NotIpv6Packet,
    #[error("expected an Icmpv4Packet payload")]
    NotIcmpv4Packet,
    #[error("expected an Icmpv6Packet")]
    NotIcmpv6Packet,
    #[error("payload too short, got {got}, want {want}")]
    PayloadTooShort { got: usize, want: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surge_error_display() {
        let err = SurgeError::IncorrectBufferSize;
        assert_eq!(err.to_string(), "buffer size was too small");

        let err = SurgeError::NetworkError;
        assert_eq!(err.to_string(), "Network error.");

        let err = SurgeError::EchoRequestPacket;
        assert_eq!(err.to_string(), "Echo Request packet.");

        let err = SurgeError::ClientDestroyed;
        assert_eq!(
            err.to_string(),
            "Client has been destroyed, ping operations are no longer available"
        );
    }

    #[test]
    fn test_surge_error_timeout() {
        let err = SurgeError::Timeout { seq: PingSequence(5) };
        assert_eq!(err.to_string(), "Request timeout for icmp_seq 5");
    }

    #[test]
    fn test_surge_error_identical_requests() {
        let host: IpAddr = "192.168.1.1".parse().unwrap();
        let err = SurgeError::IdenticalRequests {
            host,
            ident: Some(PingIdentifier(42)),
            seq: PingSequence(10),
        };
        assert_eq!(
            err.to_string(),
            "Multiple identical request"
        );
    }

    #[test]
    fn test_malformed_packet_error_display() {
        let err = MalformedPacketError::NotIpv4Packet;
        assert_eq!(err.to_string(), "expected an Ipv4Packet");

        let err = MalformedPacketError::NotIpv6Packet;
        assert_eq!(err.to_string(), "expected an Ipv6Packet");

        let err = MalformedPacketError::NotIcmpv4Packet;
        assert_eq!(err.to_string(), "expected an Icmpv4Packet payload");

        let err = MalformedPacketError::NotIcmpv6Packet;
        assert_eq!(err.to_string(), "expected an Icmpv6Packet");
    }

    #[test]
    fn test_malformed_packet_error_payload_too_short() {
        let err = MalformedPacketError::PayloadTooShort { got: 10, want: 20 };
        assert_eq!(err.to_string(), "payload too short, got 10, want 20");
    }

    #[test]
    fn test_surge_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "test");
        let surge_err: SurgeError = io_err.into();
        assert!(matches!(surge_err, SurgeError::IOError(_)));
    }

    #[test]
    fn test_surge_error_from_malformed_packet() {
        let malformed_err = MalformedPacketError::NotIpv4Packet;
        let surge_err: SurgeError = malformed_err.into();
        assert!(matches!(surge_err, SurgeError::MalformedPacket(_)));
    }
}
