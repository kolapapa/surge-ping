#[cfg(unix)]
use std::os::unix::io::{FromRawFd, IntoRawFd};
#[cfg(windows)]
use std::os::windows::io::{FromRawSocket, IntoRawSocket};

use std::{
    collections::HashMap,
    convert::TryInto,
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Instant,
};

use pnet_packet::{icmp, icmpv6, ipv4, Packet};
use rand::random;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{
    net::UdpSocket,
    sync::{mpsc, Mutex},
    task::{self, JoinHandle},
};
use tracing::warn;

use crate::{config::Config, Pinger, ICMP};

pub(crate) struct Message {
    pub when: Instant,
    pub packet: Vec<u8>,
}

impl Message {
    pub(crate) fn new(when: Instant, packet: Vec<u8>) -> Self {
        Self { when, packet }
    }
}

#[derive(Clone)]
pub(crate) struct AsyncSocket {
    inner: Arc<UdpSocket>,
}

impl AsyncSocket {
    pub(crate) fn new(config: &Config) -> io::Result<Self> {
        let socket = match config.kind {
            ICMP::V4 => Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?,
            ICMP::V6 => Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))?,
        };
        socket.set_nonblocking(true)?;
        if let Some(sock_addr) = &config.bind {
            socket.bind(sock_addr)?;
        }
        #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
        if let Some(interface) = &config.interface {
            socket.bind_device(Some(interface.as_bytes()))?;
        }
        if let Some(ttl) = config.ttl {
            socket.set_ttl(ttl)?;
        }
        #[cfg(target_os = "freebsd")]
        if let Some(fib) = config.fib {
            socket.set_fib(fib)?;
        }
        #[cfg(windows)]
        let socket = UdpSocket::from_std(unsafe {
            std::net::UdpSocket::from_raw_socket(socket.into_raw_socket())
        })?;
        #[cfg(unix)]
        let socket =
            UdpSocket::from_std(unsafe { std::net::UdpSocket::from_raw_fd(socket.into_raw_fd()) })?;
        Ok(Self {
            inner: Arc::new(socket),
        })
    }

    pub(crate) async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }

    pub(crate) async fn send_to(&self, buf: &mut [u8], target: &SocketAddr) -> io::Result<usize> {
        self.inner.send_to(buf, target).await
    }
}

pub(crate) type UniqueId = [u8; 16];
pub(crate) type ClientMapping = Arc<Mutex<HashMap<UniqueId, mpsc::Sender<Message>>>>;
///
/// If you want to pass the `Client` in the task, please wrap it with `Arc`: `Arc<Client>`.
/// and can realize the simultaneous ping of multiple addresses when only one `socket` is created.
///
#[derive(Clone)]
pub struct Client {
    socket: AsyncSocket,
    mapping: ClientMapping,
    recv: Arc<JoinHandle<()>>,
}

impl Drop for Client {
    fn drop(&mut self) {
        // The client may pass through multiple tasks, so need to judge whether the number of references is 1.
        if Arc::strong_count(&self.recv) <= 1 {
            self.recv.abort();
        }
    }
}

impl Client {
    /// A client is generated according to the configuration. In fact, a `AsyncSocket` is wrapped inside,
    /// and you can clone to any `task` at will.
    pub fn new(config: &Config) -> io::Result<Self> {
        let socket = AsyncSocket::new(config)?;
        let mapping = Arc::new(Mutex::new(HashMap::new()));
        let recv = task::spawn(recv_task(socket.clone(), mapping.clone()));

        Ok(Self {
            socket,
            mapping,
            recv: Arc::new(recv),
        })
    }

    /// Create a `Pinger` instance, you can make special configuration for this instance. Such as `timeout`, `size` etc.
    pub async fn pinger(&self, host: IpAddr) -> Pinger {
        let (tx, rx) = mpsc::channel(10);
        let key: UniqueId = random();
        {
            self.mapping.lock().await.insert(key, tx);
        }
        Pinger::new(host, self.socket.clone(), rx, key, self.mapping.clone())
    }
}

async fn recv_task(socket: AsyncSocket, mapping: ClientMapping) {
    let mut buf = [0; 2048];

    loop {
        if let Ok((sz, addr)) = socket.recv_from(&mut buf).await {
            let datas = buf[0..sz].to_vec();
            if let Some(uid) = gen_uid_with_payload(addr.ip(), datas.as_slice()) {
                let instant = Instant::now();
                let mut w = mapping.lock().await;
                if let Some(tx) = (*w).get(&uid) {
                    if tx.send(Message::new(instant, datas)).await.is_err() {
                        warn!("Pinger({}) already closed.", addr);
                        (*w).remove(&uid);
                    }
                }
            }
        }
    }
}

fn gen_uid_with_payload(addr: IpAddr, datas: &[u8]) -> Option<UniqueId> {
    match addr {
        IpAddr::V4(_) => {
            if let Some(ip_packet) = ipv4::Ipv4Packet::new(datas) {
                if let Some(icmp_packet) = icmp::IcmpPacket::new(ip_packet.payload()) {
                    let payload = icmp_packet.payload();

                    if payload.len() < 20 {
                        return None;
                    }

                    let uid = &payload[4..20];
                    return uid.try_into().ok();
                }
            }
        }
        IpAddr::V6(_) => {
            if let Some(icmpv6_packet) = icmpv6::Icmpv6Packet::new(datas) {
                let payload = icmpv6_packet.payload();

                if payload.len() < 20 {
                    return None;
                }

                let uid = &payload[4..20];
                return uid.try_into().ok();
            }
        }
    }
    None
}
