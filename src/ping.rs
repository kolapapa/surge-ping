use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use rand::random;
use tokio::{
    sync::{broadcast, mpsc, Mutex as TokioMutex},
    task,
    time::timeout,
};
use tracing::warn;
use uuid::Uuid;

use crate::client::{AsyncSocket, Message};
use crate::error::{Result, SurgeError};
use crate::icmp::{icmpv4, icmpv6, IcmpPacket};

type Token = (u16, u16);

#[derive(Debug, Clone)]
struct Cache {
    inner: Arc<Mutex<HashMap<Token, Instant>>>,
}

impl Cache {
    fn new() -> Cache {
        Cache {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn insert(&self, ident: u16, seq_cnt: u16, time: Instant) {
        self.inner.lock().insert((ident, seq_cnt), time);
    }

    fn remove(&self, ident: u16, seq_cnt: u16) -> Option<Instant> {
        self.inner.lock().remove(&(ident, seq_cnt))
    }
}

/// A Ping struct represents the state of one particular ping instance.
pub struct Pinger {
    pub destination: IpAddr,
    pub ident: u16,
    pub size: usize,
    timeout: Duration,
    socket: AsyncSocket,
    rx: mpsc::Receiver<Message>,
    cache: Cache,
    key: Uuid,
    clear_tx: broadcast::Sender<()>,
}

impl Drop for Pinger {
    fn drop(&mut self) {
        if self.clear_tx.send(()).is_err() {
            warn!("Clear Pinger cache failed");
        }
    }
}

impl Pinger {
    pub(crate) fn new(
        host: IpAddr,
        socket: AsyncSocket,
        rx: mpsc::Receiver<Message>,
        key: Uuid,
        mapping: Arc<TokioMutex<HashMap<Uuid, mpsc::Sender<Message>>>>,
    ) -> Pinger {
        let (clear_tx, _) = broadcast::channel(1);
        task::spawn(clear_mapping_key(key, mapping, clear_tx.subscribe()));
        Pinger {
            destination: host,
            ident: random(),
            size: 56,
            timeout: Duration::from_secs(2),
            socket,
            rx,
            cache: Cache::new(),
            key,
            clear_tx,
        }
    }

    /// Set the identification of ICMP.
    pub fn ident(&mut self, val: u16) -> &mut Pinger {
        self.ident = val;
        self
    }

    /// Set the packet payload size, minimal is 16. (default: 56)
    pub fn size(&mut self, size: usize) -> &mut Pinger {
        self.size = if size < 16 { 16 } else { size };
        self
    }

    /// The timeout of each Ping, in seconds. (default: 2s)
    pub fn timeout(&mut self, timeout: Duration) -> &mut Pinger {
        self.timeout = timeout;
        self
    }

    async fn recv_reply(&mut self, seq_cnt: u16) -> Result<(IcmpPacket, Duration)> {
        loop {
            let message = self.rx.recv().await.ok_or(SurgeError::NetworkError)?;
            let packet = match self.destination {
                IpAddr::V4(_) => icmpv4::Icmpv4Packet::decode(&message.packet).map(IcmpPacket::V4),
                IpAddr::V6(a) => {
                    icmpv6::Icmpv6Packet::decode(&message.packet, a).map(IcmpPacket::V6)
                }
            };
            match packet {
                Ok(packet) => {
                    if packet.check_reply_packet(self.destination, seq_cnt, self.ident) {
                        if let Some(ins) = self.cache.remove(self.ident, seq_cnt) {
                            return Ok((packet, message.when - ins));
                        }
                    }
                }
                Err(SurgeError::EchoRequestPacket) => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Send Ping request with sequence number.
    pub async fn ping(&mut self, seq_cnt: u16) -> Result<(IcmpPacket, Duration)> {
        let sender = self.socket.clone();
        let mut packet = match self.destination {
            IpAddr::V4(_) => icmpv4::make_icmpv4_echo_packet(
                self.ident,
                seq_cnt,
                self.size,
                self.key.as_bytes(),
            )?,
            IpAddr::V6(_) => icmpv6::make_icmpv6_echo_packet(
                self.ident,
                seq_cnt,
                self.size,
                self.key.as_bytes(),
            )?,
        };
        // let mut packet = EchoRequest::new(self.host, self.ident, seq_cnt, self.size).encode()?;
        let sock_addr = SocketAddr::new(self.destination, 0);
        let ident = self.ident;

        sender.send_to(&mut packet, &sock_addr).await?;
        self.cache.insert(ident, seq_cnt, Instant::now());

        match timeout(self.timeout, self.recv_reply(seq_cnt)).await {
            Ok(reply) => reply.map_err(|err| {
                self.cache.remove(ident, seq_cnt);
                err
            }),
            Err(_) => {
                self.cache.remove(ident, seq_cnt);
                Err(SurgeError::Timeout { seq: seq_cnt })
            }
        }
    }
}

async fn clear_mapping_key(
    key: Uuid,
    mapping: Arc<TokioMutex<HashMap<Uuid, mpsc::Sender<Message>>>>,
    mut rx: broadcast::Receiver<()>,
) {
    if rx.recv().await.is_ok() {
        mapping.lock().await.remove(&key);
    }
}
