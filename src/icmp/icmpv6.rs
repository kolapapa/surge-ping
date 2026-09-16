use std::convert::TryInto;
use std::net::Ipv6Addr;

use pnet_packet::Packet;
use pnet_packet::PacketSize;
use pnet_packet::icmpv6::{self, Icmpv6Code, Icmpv6Type};
use pnet_packet::ipv6;

use crate::error::{MalformedPacketError, Result, SurgeError};

use super::{PingIdentifier, PingSequence};

pub fn make_icmpv6_echo_packet(
    ident: PingIdentifier,
    seq_cnt: PingSequence,
    payload: &[u8],
) -> Result<Vec<u8>> {
    let mut buf = vec![0; 8 + payload.len()]; // 8 bytes of header, then payload
    {
        let mut packet = icmpv6::echo_request::MutableEchoRequestPacket::new(&mut buf[..])
            .ok_or(SurgeError::IncorrectBufferSize)?;
        packet.set_icmpv6_type(icmpv6::Icmpv6Types::EchoRequest);
        packet.set_identifier(ident.into_u16());
        packet.set_sequence_number(seq_cnt.into_u16());
        packet.set_payload(payload);

        // Per https://tools.ietf.org/html/rfc3542#section-3.1 the checksum is
        // omitted, the kernel will insert it.
    }

    // The packet was written into `buf` in place, so hand that back rather than
    // copying the whole datagram out of it a second time.
    Ok(buf)
}

const IPPROTO_ICMPV6: u8 = 58;

/// Walk past the extension headers of `packet`, an IPv6 packet, and return the
/// offset of the upper-layer header together with its protocol number.
///
/// Extension headers sit between the fixed 40-byte base header and the payload,
/// so the upper-layer header is not at a fixed offset. `None` is returned for a
/// chain that is truncated, absurdly long, or that ends in a header whose
/// length cannot be determined (ESP).
fn upper_layer_header(packet: &[u8]) -> Option<(usize, u8)> {
    const HOP_BY_HOP: u8 = 0;
    const ROUTING: u8 = 43;
    const FRAGMENT: u8 = 44;
    const AUTH: u8 = 51;
    const DEST_OPTIONS: u8 = 60;
    const MOBILITY: u8 = 135;
    /// Chains this long are not seen in practice; refuse rather than spin.
    const MAX_HEADERS: usize = 8;

    // The base header's Next Header field.
    let mut next = *packet.get(6)?;
    let mut offset = 40;

    for _ in 0..MAX_HEADERS {
        let len = match next {
            HOP_BY_HOP | ROUTING | DEST_OPTIONS | MOBILITY => {
                // Header Extension Length counts 8-octet units, excluding the first.
                (*packet.get(offset + 1)? as usize + 1) * 8
            }
            FRAGMENT => 8,
            // The Authentication Header measures in 4-octet units, excluding two.
            AUTH => (*packet.get(offset + 1)? as usize + 2) * 4,
            // Anything else is the upper-layer protocol.
            _ => return Some((offset, next)),
        };
        // This header's own Next Header field, read before moving past it.
        next = *packet.get(offset)?;
        offset = offset.checked_add(len)?;
    }
    None
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
    real_dest: Ipv6Addr,
    identifier: PingIdentifier,
    sequence: PingSequence,
}

impl Default for Icmpv6Packet {
    fn default() -> Self {
        Icmpv6Packet {
            source: Ipv6Addr::LOCALHOST,
            destination: Ipv6Addr::LOCALHOST,
            max_hop_limit: 0,
            icmpv6_type: Icmpv6Type::new(0),
            icmpv6_code: Icmpv6Code::new(0),
            size: 0,
            real_dest: Ipv6Addr::LOCALHOST,
            identifier: PingIdentifier(0),
            sequence: PingSequence(0),
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

    fn real_dest(&mut self, addr: Ipv6Addr) -> &mut Self {
        self.real_dest = addr;
        self
    }

    /// If it is an `echo_reply` packet, it is the source address in the IPv6 packet.
    /// If it is other packets, it is the destination address in the IPv6 packet in ICMPv6's payload.
    pub fn get_real_dest(&self) -> Ipv6Addr {
        self.real_dest
    }

    fn identifier(&mut self, identifier: PingIdentifier) -> &mut Self {
        self.identifier = identifier;
        self
    }

    /// Get the identifier of the icmp_v6 packet.
    pub fn get_identifier(&self) -> PingIdentifier {
        self.identifier
    }

    fn sequence(&mut self, sequence: PingSequence) -> &mut Self {
        self.sequence = sequence;
        self
    }

    /// Get the sequence of the icmp_v6 packet.
    pub fn get_sequence(&self) -> PingSequence {
        self.sequence
    }

    /// Decode into icmpv6 packet from the socket message.
    pub fn decode(buf: &[u8], destination: Ipv6Addr) -> Result<Self> {
        // The IPv6 header is automatically cropped off when recvfrom() is used.
        let icmpv6_packet = icmpv6::Icmpv6Packet::new(buf)
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpv6Packet))?;
        let icmpv6_payload = icmpv6_packet.payload();
        match icmpv6_packet.get_icmpv6_type() {
            icmpv6::Icmpv6Types::EchoRequest => Err(SurgeError::EchoRequestPacket),
            icmpv6::Icmpv6Types::EchoReply => {
                if icmpv6_payload.len() < 4 {
                    return Err(SurgeError::from(MalformedPacketError::PayloadTooShort {
                        got: icmpv6_payload.len(),
                        want: 4,
                    }));
                }
                let identifier = u16::from_be_bytes(icmpv6_payload[0..2].try_into().unwrap());
                let sequence = u16::from_be_bytes(icmpv6_payload[2..4].try_into().unwrap());
                let mut packet = Icmpv6Packet::default();
                packet
                    .source(destination)
                    .destination(Ipv6Addr::LOCALHOST)
                    .max_hop_limit(0)
                    .icmpv6_type(icmpv6_packet.get_icmpv6_type())
                    .icmpv6_code(icmpv6_packet.get_icmpv6_code())
                    .size(icmpv6_packet.packet().len())
                    .real_dest(destination)
                    .identifier(identifier.into())
                    .sequence(sequence.into());
                Ok(packet)
            }
            _ => {
                // The error quotes the packet that triggered it: unused(4)
                // followed by that packet's IPv6 base header.
                const MIN_LEN: usize = 4 + 40;
                if icmpv6_payload.len() < MIN_LEN {
                    return Err(SurgeError::from(MalformedPacketError::PayloadTooShort {
                        got: icmpv6_payload.len(),
                        want: MIN_LEN,
                    }));
                }
                let quoted = &icmpv6_payload[4..];
                // Its destination is the host the caller asked us to ping, and
                // it lives in the base header.
                let real_ip_packet = ipv6::Ipv6Packet::new(quoted)
                    .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIpv6Packet))?;

                // The quoted echo header follows any extension headers.
                let (echo, protocol) = upper_layer_header(quoted)
                    .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIpv6Packet))?;
                if protocol != IPPROTO_ICMPV6 {
                    // Not an error about one of our echo requests.
                    return Err(SurgeError::from(MalformedPacketError::NotIcmpv6Packet));
                }
                if quoted.len() < echo + 8 {
                    return Err(SurgeError::from(MalformedPacketError::PayloadTooShort {
                        got: icmpv6_payload.len(),
                        want: 4 + echo + 8,
                    }));
                }

                // Skip the quoted echo header's type, code and checksum.
                let identifier = u16::from_be_bytes(quoted[echo + 4..echo + 6].try_into().unwrap());
                let sequence = u16::from_be_bytes(quoted[echo + 6..echo + 8].try_into().unwrap());
                let mut packet = Icmpv6Packet::default();
                packet
                    .source(destination)
                    .destination(destination)
                    .max_hop_limit(0)
                    .icmpv6_type(icmpv6_packet.get_icmpv6_type())
                    .icmpv6_code(icmpv6_packet.get_icmpv6_code())
                    .size(icmpv6_packet.packet_size())
                    .real_dest(real_ip_packet.get_destination())
                    .identifier(identifier.into())
                    .sequence(sequence.into());
                Ok(packet)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As for IPv4: the builder returns its own buffer, and the result must
    /// match what `packet().to_vec()` used to copy out of it.
    #[test]
    fn echo_request_is_built_in_place() {
        let payload: Vec<u8> = (0u8..32).collect();
        let buf =
            make_icmpv6_echo_packet(PingIdentifier(0x1234), PingSequence(9), &payload).unwrap();

        assert_eq!(buf.len(), 8 + payload.len(), "no bytes lost or added");
        assert_eq!(buf[0], 128, "echo request type");
        assert_eq!(buf[1], 0, "code");
        assert_eq!(
            &buf[2..4],
            &[0, 0],
            "checksum is left to the kernel, per RFC 3542 section 3.1"
        );
        assert_eq!(&buf[4..6], &0x1234u16.to_be_bytes(), "identifier");
        assert_eq!(&buf[6..8], &9u16.to_be_bytes(), "sequence");
        assert_eq!(&buf[8..], &payload[..], "payload verbatim");

        let mut copied = vec![0u8; 8 + payload.len()];
        {
            let mut p =
                icmpv6::echo_request::MutableEchoRequestPacket::new(&mut copied[..]).unwrap();
            p.set_icmpv6_type(icmpv6::Icmpv6Types::EchoRequest);
            p.set_identifier(0x1234);
            p.set_sequence_number(9);
            p.set_payload(&payload);
            assert_eq!(
                p.packet().len(),
                8 + payload.len(),
                "packet() must span the whole buffer, or returning it would truncate"
            );
        }
        assert_eq!(buf, copied, "identical to the previous implementation");
    }

    #[test]
    fn echo_request_accepts_an_empty_payload() {
        let buf = make_icmpv6_echo_packet(PingIdentifier(1), PingSequence(2), &[]).unwrap();
        assert_eq!(buf.len(), 8);
        assert_eq!(&buf[6..8], &2u16.to_be_bytes());
    }

    /// An ICMPv6 Time Exceeded quoting an echo request addressed to `target`.
    /// `extensions` are extension headers carried by the quoted request, given
    /// as (protocol number, header bytes) pairs and chained in order; they push
    /// the quoted echo header past its usual offset.
    fn time_exceeded_with_extensions(
        target: Ipv6Addr,
        ident: u16,
        seq: u16,
        extensions: &[(u8, Vec<u8>)],
    ) -> Vec<u8> {
        let mut icmp = vec![3, 0, 0, 0]; // type = time exceeded, code, checksum
        icmp.extend_from_slice(&[0, 0, 0, 0]); // unused
        // The quoted 40-byte IPv6 base header of the original request.
        icmp.extend_from_slice(&[0x60, 0, 0, 0]); // version / traffic class / flow label
        icmp.extend_from_slice(&[0, 8]); // payload length
        // Next Header points at the first extension header, or straight at ICMPv6.
        let first = extensions.first().map(|(proto, _)| *proto).unwrap_or(58);
        icmp.extend_from_slice(&[first, 64]); // next header, hop limit
        icmp.extend_from_slice(&Ipv6Addr::LOCALHOST.octets()); // source: us
        icmp.extend_from_slice(&target.octets()); // destination: the ping target

        // Each extension header starts with the protocol of the one after it.
        for (idx, (_, body)) in extensions.iter().enumerate() {
            let next = extensions
                .get(idx + 1)
                .map(|(proto, _)| *proto)
                .unwrap_or(58);
            icmp.push(next);
            icmp.extend_from_slice(body);
        }

        // The quoted first 8 bytes of the original echo request.
        icmp.extend_from_slice(&[128, 0, 0, 0]);
        icmp.extend_from_slice(&ident.to_be_bytes());
        icmp.extend_from_slice(&seq.to_be_bytes());
        icmp
    }

    fn time_exceeded(target: Ipv6Addr, ident: u16, seq: u16) -> Vec<u8> {
        time_exceeded_with_extensions(target, ident, seq, &[])
    }

    /// A Destination Options header: 8 bytes total, so Header Extension Length
    /// is 0. The leading Next Header byte is supplied by the chain builder.
    fn dest_options() -> (u8, Vec<u8>) {
        (60, vec![0, 1, 4, 0, 0, 0, 0])
    }

    /// The error is sent by a router, so both the target and the sequence have
    /// to come out of the quoted request for the reply to reach its waiter.
    #[test]
    fn time_exceeded_reports_the_quoted_target_and_sequence() {
        let router: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let target: Ipv6Addr = "2001:4860:4860::8888".parse().unwrap();

        let packet = Icmpv6Packet::decode(&time_exceeded(target, 0x1234, 7), router).unwrap();

        assert_eq!(packet.get_source(), router);
        assert_eq!(packet.get_real_dest(), target, "must route to the target");
        assert_eq!(packet.get_identifier(), PingIdentifier(0x1234));
        assert_eq!(packet.get_sequence(), PingSequence(7));
    }

    /// Extension headers sit between the base header and the echo header, so
    /// reading the identifier and sequence at a fixed offset used to pick up
    /// extension-header bytes instead.
    #[test]
    fn time_exceeded_walks_extension_headers() {
        let router: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let target: Ipv6Addr = "2001:4860:4860::8888".parse().unwrap();

        // One extension header, then two chained.
        for chain in [vec![dest_options()], vec![dest_options(), dest_options()]] {
            let raw = time_exceeded_with_extensions(target, 0xbeef, 300, &chain);
            let packet = Icmpv6Packet::decode(&raw, router).unwrap();

            assert_eq!(packet.get_real_dest(), target);
            assert_eq!(packet.get_identifier(), PingIdentifier(0xbeef));
            assert_eq!(packet.get_sequence(), PingSequence(300));
        }
    }

    /// A quoted packet that is not ICMPv6 is not an error about a ping of ours.
    #[test]
    fn an_error_quoting_a_non_icmpv6_packet_is_rejected() {
        let target: Ipv6Addr = "2001:4860:4860::8888".parse().unwrap();
        let mut raw = time_exceeded(target, 1, 1);
        // icmpv6 header(4) + unused(4), then the quoted base header's Next Header.
        raw[4 + 4 + 6] = 17; // UDP

        assert!(Icmpv6Packet::decode(&raw, "2001:db8::1".parse().unwrap()).is_err());
    }

    /// An extension header chain that runs off the end of the quoted bytes must
    /// be rejected rather than read out of bounds.
    #[test]
    fn a_truncated_extension_chain_is_rejected() {
        let router: Ipv6Addr = "2001:db8::1".parse().unwrap();
        let target: Ipv6Addr = "2001:4860:4860::8888".parse().unwrap();
        let full = time_exceeded_with_extensions(target, 1, 1, &[dest_options()]);

        for len in [44, 50, 56, 59] {
            assert!(
                Icmpv6Packet::decode(&full[..len], router).is_err(),
                "a {len}-byte error message must not decode"
            );
        }
        assert!(Icmpv6Packet::decode(&full, router).is_ok());
    }

    #[test]
    fn a_truncated_error_is_rejected() {
        let target: Ipv6Addr = "2001:4860:4860::8888".parse().unwrap();
        let full = time_exceeded(target, 1, 1);
        for len in [4, 12, 44, 51] {
            assert!(
                Icmpv6Packet::decode(&full[..len], "2001:db8::1".parse().unwrap()).is_err(),
                "a {len}-byte error message must not decode"
            );
        }
    }

    #[test]
    fn echo_reply_real_dest_is_the_sender() {
        let host: Ipv6Addr = "2001:4860:4860::8888".parse().unwrap();
        let mut reply = vec![129, 0, 0, 0]; // echo reply, code, checksum
        reply.extend_from_slice(&0x1234u16.to_be_bytes());
        reply.extend_from_slice(&9u16.to_be_bytes());

        let packet = Icmpv6Packet::decode(&reply, host).unwrap();
        assert_eq!(packet.get_real_dest(), host);
        assert_eq!(packet.get_identifier(), PingIdentifier(0x1234));
        assert_eq!(packet.get_sequence(), PingSequence(9));
    }
}
