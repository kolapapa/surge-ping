use std::{
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

use tokio::time::timeout;

use crate::{
    client::{AsyncSocket, ReplyMap},
    error::{Result, SurgeError},
    icmp::{icmpv4, icmpv6, IcmpPacket, PingIdentifier, PingSequence},
    is_linux_icmp_socket,
};

/// A Ping struct represents the state of one particular ping instance.
pub struct Pinger {
    pub host: IpAddr,
    pub ident: Option<PingIdentifier>,
    timeout: Duration,
    socket: AsyncSocket,
    reply_map: ReplyMap,
    last_sequence: Option<PingSequence>,
}

impl Drop for Pinger {
    fn drop(&mut self) {
        if let Some(sequence) = self.last_sequence.take() {
            // Ensure no reply waiter is left hanging if this pinger is dropped while
            // waiting for a reply.
            self.reply_map.remove(self.host, self.ident, sequence);
        }
    }
}

impl Pinger {
    pub(crate) fn new(
        host: IpAddr,
        ident_hint: PingIdentifier,
        socket: AsyncSocket,
        response_map: ReplyMap,
    ) -> Pinger {
        let ident;
        if is_linux_icmp_socket!(socket.get_type()) {
            ident = None;
        } else {
            ident = Some(ident_hint);
        }

        Pinger {
            host,
            ident,
            timeout: Duration::from_secs(2),
            socket,
            reply_map: response_map,
            last_sequence: None,
        }
    }

    /// The timeout of each Ping, in seconds. (default: 2s)
    pub fn timeout(&mut self, timeout: Duration) -> &mut Pinger {
        self.timeout = timeout;
        self
    }

    /// Send Ping request with sequence number.
    pub async fn ping(
        &mut self,
        seq: PingSequence,
        payload: &[u8],
    ) -> Result<(IcmpPacket, Duration)> {
        // Register to wait for a reply.
        let reply_waiter = self.reply_map.new_waiter(self.host, self.ident, seq)?;

        // Create and send ping packet.
        let mut packet = match self.host {
            IpAddr::V4(_) => icmpv4::make_icmpv4_echo_packet(
                self.ident.unwrap_or(PingIdentifier(0)),
                seq,
                self.socket.get_type(),
                payload,
            )?,
            IpAddr::V6(_) => icmpv6::make_icmpv6_echo_packet(
                self.ident.unwrap_or(PingIdentifier(0)),
                seq,
                payload,
            )?,
        };

        self.socket
            .send_to(&mut packet, &SocketAddr::new(self.host, 0))
            .await?;
        let send_time = Instant::now();
        self.last_sequence = Some(seq);

        // Wait for reply or timeout.
        let result = match timeout(self.timeout, reply_waiter).await {
            Ok(Ok(reply)) => Ok((
                reply.packet,
                reply.timestamp.saturating_duration_since(send_time),
            )),
            Ok(Err(_err)) => Err(SurgeError::NetworkError),
            Err(_) => {
                self.reply_map.remove(self.host, self.ident, seq);
                Err(SurgeError::Timeout { seq })
            }
        };
        result
    }
}
