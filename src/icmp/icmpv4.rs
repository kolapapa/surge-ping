use socket2::Type as SockType;
use std::convert::TryInto;
use std::net::Ipv4Addr;

use pnet_packet::Packet;
use pnet_packet::icmp::{self, IcmpCode, IcmpType};
use pnet_packet::{PacketSize, ipv4};

use crate::{
    error::{MalformedPacketError, Result, SurgeError},
    is_linux_icmp_socket,
};

use super::{PingIdentifier, PingSequence};

pub fn make_icmpv4_echo_packet(
    ident_hint: PingIdentifier,
    seq_cnt: PingSequence,
    sock_type: SockType,
    payload: &[u8],
) -> Result<Vec<u8>> {
    // 8 bytes of header, then payload.
    let mut buf = vec![0; 8 + payload.len()];
    let mut packet = icmp::echo_request::MutableEchoRequestPacket::new(&mut buf[..])
        .ok_or(SurgeError::IncorrectBufferSize)?;

    packet.set_icmp_type(icmp::IcmpTypes::EchoRequest);
    packet.set_payload(payload);
    packet.set_sequence_number(seq_cnt.into_u16());

    if !(is_linux_icmp_socket!(sock_type)) {
        packet.set_identifier(ident_hint.into_u16());

        // Calculate and set the checksum
        let icmp_packet =
            icmp::IcmpPacket::new(packet.packet()).ok_or(SurgeError::IncorrectBufferSize)?;

        let checksum = icmp::checksum(&icmp_packet);
        packet.set_checksum(checksum);
    }

    Ok(packet.packet().to_vec())
}

/// Read the request quoted inside an ICMP error message.
///
/// `icmp_payload` starts at the error's unused/MTU word, followed by the IPv4
/// header of the packet that triggered the error and the first bytes of that
/// packet's payload. The quoted header may carry options, so the echo header
/// does not sit at a fixed offset: its position follows from the quoted IHL.
///
/// Returns the address the original request was sent to, plus its identifier
/// and sequence number.
fn parse_quoted_request(icmp_payload: &[u8]) -> Result<(Ipv4Addr, PingIdentifier, PingSequence)> {
    // unused(4) + the smallest possible IPv4 header(20) + echo header(8)
    const MIN_LEN: usize = 4 + 20 + 8;
    if icmp_payload.len() < MIN_LEN {
        return Err(MalformedPacketError::PayloadTooShort {
            got: icmp_payload.len(),
            want: MIN_LEN,
        }
        .into());
    }

    let quoted =
        ipv4::Ipv4Packet::new(&icmp_payload[4..]).ok_or(MalformedPacketError::NotIpv4Packet)?;
    let header_len = quoted.get_header_length() as usize * 4;
    if header_len < 20 {
        return Err(MalformedPacketError::NotIpv4Packet.into());
    }

    let echo = 4 + header_len;
    if icmp_payload.len() < echo + 8 {
        return Err(MalformedPacketError::PayloadTooShort {
            got: icmp_payload.len(),
            want: echo + 8,
        }
        .into());
    }

    // Skip the quoted echo header's type, code and checksum.
    let identifier = u16::from_be_bytes(icmp_payload[echo + 4..echo + 6].try_into().unwrap());
    let sequence = u16::from_be_bytes(icmp_payload[echo + 6..echo + 8].try_into().unwrap());

    Ok((quoted.get_destination(), identifier.into(), sequence.into()))
}

/// Packet structure returned by ICMPv4.
#[derive(Debug)]
pub struct Icmpv4Packet {
    source: Ipv4Addr,
    destination: Ipv4Addr,
    ttl: Option<u8>,
    icmp_type: IcmpType,
    icmp_code: IcmpCode,
    size: usize,
    real_dest: Ipv4Addr,
    identifier: PingIdentifier,
    sequence: PingSequence,
}

impl Default for Icmpv4Packet {
    fn default() -> Self {
        Icmpv4Packet {
            source: Ipv4Addr::new(127, 0, 0, 1),
            destination: Ipv4Addr::new(127, 0, 0, 1),
            ttl: None,
            icmp_type: IcmpType::new(0),
            icmp_code: IcmpCode::new(0),
            size: 0,
            real_dest: Ipv4Addr::new(127, 0, 0, 1),
            identifier: PingIdentifier(0),
            sequence: PingSequence(0),
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
        self.ttl = Some(ttl);
        self
    }

    /// Get the ttl field.
    pub fn get_ttl(&self) -> Option<u8> {
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

    fn identifier(&mut self, identifier: PingIdentifier) -> &mut Self {
        self.identifier = identifier;
        self
    }

    /// Get the identifier of the icmp_v4 packet.
    pub fn get_identifier(&self) -> PingIdentifier {
        self.identifier
    }

    fn sequence(&mut self, sequence: PingSequence) -> &mut Self {
        self.sequence = sequence;
        self
    }

    /// Get the sequence of the icmp_v4 packet.
    pub fn get_sequence(&self) -> PingSequence {
        self.sequence
    }

    /// Decode into icmp packet from the socket message.
    pub fn decode(
        buf: &[u8],
        sock_type: SockType,
        src_addr: Ipv4Addr,
        dst_addr: Ipv4Addr,
    ) -> Result<Self> {
        if is_linux_icmp_socket!(sock_type) {
            Self::decode_from_icmp(buf, src_addr, dst_addr)
        } else {
            Self::decode_from_ipv4(buf)
        }
    }

    fn decode_from_ipv4(buf: &[u8]) -> Result<Self> {
        let ipv4_packet = ipv4::Ipv4Packet::new(buf)
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIpv4Packet))?;
        let icmp_packet = icmp::IcmpPacket::new(ipv4_packet.payload())
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpv4Packet))?;
        let mut packet = Icmpv4Packet::default();

        match icmp_packet.get_icmp_type() {
            icmp::IcmpTypes::EchoReply => {
                let icmp_packet = icmp::echo_reply::EchoReplyPacket::new(icmp_packet.packet())
                    .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpv4Packet))?;

                packet
                    .source(ipv4_packet.get_source())
                    .destination(ipv4_packet.get_destination())
                    .ttl(ipv4_packet.get_ttl())
                    .icmp_type(icmp_packet.get_icmp_type())
                    .icmp_code(icmp_packet.get_icmp_code())
                    .size(icmp_packet.packet().len())
                    .real_dest(ipv4_packet.get_source())
                    .identifier(icmp_packet.get_identifier().into())
                    .sequence(icmp_packet.get_sequence_number().into());
            }
            icmp::IcmpTypes::EchoRequest => return Err(SurgeError::EchoRequestPacket),
            _ => {
                let (real_dest, identifier, sequence) =
                    parse_quoted_request(icmp_packet.payload())?;

                packet
                    .source(ipv4_packet.get_source())
                    .destination(ipv4_packet.get_destination())
                    .ttl(ipv4_packet.get_ttl())
                    .icmp_type(icmp_packet.get_icmp_type())
                    .icmp_code(icmp_packet.get_icmp_code())
                    .size(icmp_packet.packet_size())
                    .real_dest(real_dest)
                    .identifier(identifier)
                    .sequence(sequence);
            }
        }

        Ok(packet)
    }

    fn decode_from_icmp(buf: &[u8], src_addr: Ipv4Addr, dst_addr: Ipv4Addr) -> Result<Self> {
        let icmp_packet = icmp::IcmpPacket::new(buf)
            .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpv4Packet))?;
        let mut packet = Icmpv4Packet::default();

        match icmp_packet.get_icmp_type() {
            icmp::IcmpTypes::EchoReply => {
                let icmp_packet = icmp::echo_reply::EchoReplyPacket::new(icmp_packet.packet())
                    .ok_or_else(|| SurgeError::from(MalformedPacketError::NotIcmpv4Packet))?;

                packet
                    .source(src_addr)
                    .destination(dst_addr)
                    .icmp_type(icmp_packet.get_icmp_type())
                    .icmp_code(icmp_packet.get_icmp_code())
                    .size(icmp_packet.packet().len())
                    .real_dest(src_addr)
                    .identifier(icmp_packet.get_identifier().into())
                    .sequence(icmp_packet.get_sequence_number().into());
            }
            icmp::IcmpTypes::EchoRequest => return Err(SurgeError::EchoRequestPacket),
            _ => {
                let (real_dest, identifier, sequence) =
                    parse_quoted_request(icmp_packet.payload())?;

                packet
                    .source(src_addr)
                    .destination(dst_addr)
                    .icmp_type(icmp_packet.get_icmp_type())
                    .icmp_code(icmp_packet.get_icmp_code())
                    .size(icmp_packet.packet_size())
                    .real_dest(real_dest)
                    .identifier(identifier)
                    .sequence(sequence);
            }
        }

        Ok(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Icmpv4Packet;

    // The two wire formats are decoded by separate functions, and which one
    // `decode` picks is platform dependent, so each fixture is handed to the
    // function that matches its format.

    const HOST: &str = "172.217.14.110";
    const US: &str = "10.0.242.34";

    #[test]
    fn malformed_packet() {
        let decoded_ipv4 =
            hex::decode("4500001d0000000079018a76acd90e6e0a00f22203006c3293cc").unwrap();
        assert!(Icmpv4Packet::decode_from_ipv4(&decoded_ipv4).is_err());

        let decoded_icmp = hex::decode("03006c3293cc").unwrap();
        assert!(
            Icmpv4Packet::decode_from_icmp(
                &decoded_icmp,
                HOST.parse().unwrap(),
                US.parse().unwrap(),
            )
            .is_err()
        );
    }

    #[test]
    fn short_packet() {
        let decoded_ipv4 =
            hex::decode("4500001d0000000079018a76acd90e6e0a00f22203006c3293cc000100").unwrap();
        assert!(Icmpv4Packet::decode_from_ipv4(&decoded_ipv4).is_err());

        let decoded_icmp = hex::decode("03006c3293cc000100").unwrap();
        assert!(
            Icmpv4Packet::decode_from_icmp(
                &decoded_icmp,
                HOST.parse().unwrap(),
                US.parse().unwrap(),
            )
            .is_err()
        );
    }

    /// An ICMP Time Exceeded quoting an echo request that was addressed to
    /// `target`, as an intermediate router would send it. `options` are IPv4
    /// header options carried by the quoted request (length must be a multiple
    /// of 4), which push the quoted echo header past its usual offset.
    fn time_exceeded_with_options(
        target: Ipv4Addr,
        ident: u16,
        seq: u16,
        options: &[u8],
    ) -> Vec<u8> {
        assert_eq!(options.len() % 4, 0, "IPv4 options are padded to 4 bytes");
        let ihl = (20 + options.len()) / 4;

        let mut icmp = vec![11, 0, 0, 0]; // type = time exceeded, code, checksum
        icmp.extend_from_slice(&[0, 0, 0, 0]); // unused
        // The quoted IPv4 header of the original request.
        icmp.extend_from_slice(&[0x40 | ihl as u8, 0, 0, 28, 0, 0, 0, 0, 1, 1, 0, 0]);
        icmp.extend_from_slice(&[192, 168, 1, 10]); // source: us
        icmp.extend_from_slice(&target.octets()); // destination: the ping target
        icmp.extend_from_slice(options);
        // The quoted first 8 bytes of the original echo request.
        icmp.extend_from_slice(&[8, 0, 0, 0]);
        icmp.extend_from_slice(&ident.to_be_bytes());
        icmp.extend_from_slice(&seq.to_be_bytes());
        icmp
    }

    fn time_exceeded(target: Ipv4Addr, ident: u16, seq: u16) -> Vec<u8> {
        time_exceeded_with_options(target, ident, seq, &[])
    }

    fn with_ipv4_header(src: Ipv4Addr, dst: Ipv4Addr, payload: &[u8]) -> Vec<u8> {
        let total = (20 + payload.len()) as u16;
        let mut buf = vec![0x45, 0];
        buf.extend_from_slice(&total.to_be_bytes());
        buf.extend_from_slice(&[0, 0, 0, 0, 64, 1, 0, 0]);
        buf.extend_from_slice(&src.octets());
        buf.extend_from_slice(&dst.octets());
        buf.extend_from_slice(payload);
        buf
    }

    /// An ICMP error comes from a router, not from the host that was pinged, so
    /// the original target has to be recovered from the quoted IP header for the
    /// reply to be routed back to the request that is waiting for it.
    #[test]
    fn time_exceeded_reports_the_quoted_target() {
        let router: Ipv4Addr = "10.0.0.1".parse().unwrap();
        let us: Ipv4Addr = "192.168.1.10".parse().unwrap();
        let target: Ipv4Addr = "8.8.8.8".parse().unwrap();
        let icmp = time_exceeded(target, 0x1234, 7);

        // Both wire formats are exercised directly: which one `decode` picks
        // depends on the platform, and this behaviour must hold for either.

        // Message carrying the IPv4 header (raw sockets, and BSD/macOS).
        let packet = Icmpv4Packet::decode_from_ipv4(&with_ipv4_header(router, us, &icmp)).unwrap();
        assert_eq!(packet.get_source(), router);
        assert_eq!(packet.get_real_dest(), target, "must route to the target");
        assert_eq!(packet.get_identifier(), PingIdentifier(0x1234));
        assert_eq!(packet.get_sequence(), PingSequence(7));

        // Message starting at the ICMP header (Linux ICMP sockets).
        let packet = Icmpv4Packet::decode_from_icmp(&icmp, router, us).unwrap();
        assert_eq!(packet.get_source(), router);
        assert_eq!(packet.get_real_dest(), target, "must route to the target");
        assert_eq!(packet.get_identifier(), PingIdentifier(0x1234));
        assert_eq!(packet.get_sequence(), PingSequence(7));
    }

    /// The quoted header carries options, so the echo header it precedes is no
    /// longer at the usual offset. Reading fixed offsets here used to return
    /// option bytes as the identifier and sequence.
    #[test]
    fn time_exceeded_honours_the_quoted_header_length() {
        let router: Ipv4Addr = "10.0.0.1".parse().unwrap();
        let us: Ipv4Addr = "192.168.1.10".parse().unwrap();
        let target: Ipv4Addr = "8.8.8.8".parse().unwrap();

        // Record Route option, 8 bytes: the quoted header is 28 bytes, not 20.
        let options = [7u8, 8, 4, 0, 0, 0, 0, 0];
        let icmp = time_exceeded_with_options(target, 0xbeef, 300, &options);

        let packet = Icmpv4Packet::decode_from_icmp(&icmp, router, us).unwrap();
        assert_eq!(packet.get_real_dest(), target);
        assert_eq!(packet.get_identifier(), PingIdentifier(0xbeef));
        assert_eq!(packet.get_sequence(), PingSequence(300));

        let packet = Icmpv4Packet::decode_from_ipv4(&with_ipv4_header(router, us, &icmp)).unwrap();
        assert_eq!(packet.get_real_dest(), target);
        assert_eq!(packet.get_identifier(), PingIdentifier(0xbeef));
        assert_eq!(packet.get_sequence(), PingSequence(300));
    }

    /// An error whose quoted header claims options that were not included must
    /// be rejected rather than read out of bounds.
    #[test]
    fn a_truncated_quoted_header_is_rejected() {
        let router: Ipv4Addr = "10.0.0.1".parse().unwrap();
        let us: Ipv4Addr = "192.168.1.10".parse().unwrap();
        let target: Ipv4Addr = "8.8.8.8".parse().unwrap();

        let full = time_exceeded_with_options(target, 1, 1, &[7, 8, 4, 0, 0, 0, 0, 0]);
        for len in [4, 20, 31, 32, 39] {
            assert!(
                Icmpv4Packet::decode_from_icmp(&full[..len], router, us).is_err(),
                "a {len}-byte error message must not decode"
            );
        }
        // The full 40-byte message does decode.
        assert!(Icmpv4Packet::decode_from_icmp(&full, router, us).is_ok());
    }

    /// A well-formed echo reply, in both wire formats. For an echo reply the
    /// sender *is* the target the request went to, so routing is unaffected.
    ///
    /// NOTE: this fixture used to carry ICMP type 3 (destination unreachable)
    /// in front of an echo reply body, which no decoder can read coherently;
    /// the type byte now matches the payload it precedes.
    #[test]
    fn standard_packet() {
        let host: Ipv4Addr = "142.250.176.14".parse().unwrap();
        let us: Ipv4Addr = US.parse().unwrap();
        let body = "4176a1ee0001613dd762000000002127040000000000101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f3031323334353637";

        let decoded_ipv4 = hex::decode(format!(
            "45000054000000007901067e8efab00e0a00f2220000{body}"
        ))
        .unwrap();
        let packet = Icmpv4Packet::decode_from_ipv4(&decoded_ipv4).unwrap();
        assert_eq!(packet.get_source(), host);
        assert_eq!(packet.get_destination(), us);
        assert_eq!(packet.get_ttl(), Some(121));
        assert_eq!(packet.get_real_dest(), host);
        assert_eq!(packet.get_identifier(), PingIdentifier(0xa1ee));
        assert_eq!(packet.get_sequence(), PingSequence(1));

        let decoded_icmp = hex::decode(format!("0000{body}")).unwrap();
        let packet = Icmpv4Packet::decode_from_icmp(&decoded_icmp, host, us).unwrap();
        assert_eq!(packet.get_source(), host);
        assert_eq!(packet.get_destination(), us);
        assert_eq!(packet.get_real_dest(), host);
        assert_eq!(packet.get_identifier(), PingIdentifier(0xa1ee));
        assert_eq!(packet.get_sequence(), PingSequence(1));
    }
}
