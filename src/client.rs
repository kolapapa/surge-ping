#[cfg(unix)]
use std::os::unix::io::{FromRawFd, IntoRawFd};
#[cfg(windows)]
use std::os::windows::io::{FromRawSocket, IntoRawSocket};

use std::{
    collections::HashMap,
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Instant,
};

use pnet_packet::{icmp, icmpv6, ipv4, ipv6, Packet};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{
    net::UdpSocket,
    sync::{mpsc, Mutex},
    task,
};
use tracing::warn;
use uuid::Uuid;

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

///
/// `Client` is a type wrapped by `Arc`, so you can `clone` arbitrarily cheaply,
/// and can realize the simultaneous ping of multiple addresses when only one `socket` is created.
///
#[derive(Clone)]
pub struct Client {
    socket: AsyncSocket,
    mapping: Arc<Mutex<HashMap<Uuid, mpsc::Sender<Message>>>>,
}

impl Client {
    /// A client is generated according to the configuration. In fact, a `AsyncSocket` is wrapped inside,
    /// and you can clone to any `task` at will.
    pub async fn new(config: &Config) -> io::Result<Self> {
        let socket = AsyncSocket::new(config)?;
        let mapping = Arc::new(Mutex::new(HashMap::new()));
        task::spawn(recv_task(socket.clone(), mapping.clone()));
        Ok(Self { socket, mapping })
    }

    /// Create a `Pinger` instance, you can make special configuration for this instance. Such as `timeout`, `size` etc.
    pub async fn pinger(&self, host: IpAddr) -> Pinger {
        let (tx, rx) = mpsc::channel(10);
        let key = Uuid::new_v4();
        {
            self.mapping.lock().await.insert(key, tx);
        }
        Pinger::new(host, self.socket.clone(), rx, key)
    }
}

async fn recv_task(socket: AsyncSocket, mapping: Arc<Mutex<HashMap<Uuid, mpsc::Sender<Message>>>>) {
    let mut buf = [0; 2048];
    loop {
        if let Ok((sz, addr)) = socket.recv_from(&mut buf).await {
            let datas = buf[0..sz].to_vec();
            if let Some(uuid) = gen_uuid_with_payload(addr.ip(), datas.as_slice()) {
                let instant = Instant::now();
                let mut w = mapping.lock().await;
                if let Some(tx) = (*w).get(&uuid) {
                    if tx.send(Message::new(instant, datas)).await.is_err() {
                        warn!("Pinger({}) already closed.", addr);
                        (*w).remove(&uuid);
                    }
                }
            }
        }
    }
}

fn gen_uuid_with_payload(addr: IpAddr, datas: &[u8]) -> Option<Uuid> {
    match addr {
        IpAddr::V4(_) => {
            if let Some(ip_packet) = ipv4::Ipv4Packet::new(datas) {
                if let Some(icmp_packet) = icmp::IcmpPacket::new(ip_packet.payload()) {
                    let payload = icmp_packet.payload();
                    if let Some(uuid) = &payload.get(4..20) {
                        return Uuid::from_slice(uuid).ok();
                    }
                }
            }
        }
        IpAddr::V6(_) => {
            if let Some(ipv6_packet) = ipv6::Ipv6Packet::new(datas) {
                if let Some(icmpv6_packet) = icmpv6::Icmpv6Packet::new(ipv6_packet.payload()) {
                    let payload = icmpv6_packet.payload();
                    if let Some(uuid) = &payload.get(4..20) {
                        return Uuid::from_slice(uuid).ok();
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use crate::client::gen_uuid_with_payload;
    use std::str::FromStr;

    #[test]
    fn short_packet() {
        let decoded = hex::decode("4500001d0000000079018a76acd90e6e0a00f22200006c3293cc000100").unwrap();

        assert!(gen_uuid_with_payload(IpAddr::from_str("127.0.0.1").unwrap(), &decoded).is_none());
    }

    #[test]
    fn standard_packet() {
        let decoded = hex::decode("45000054000000007901067e8efab00e0a00f22200004176a1ee0001613dd762000000002127040000000000101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f3031323334353637").unwrap();

        gen_uuid_with_payload(IpAddr::from_str("127.0.0.1").unwrap(), &decoded).unwrap();
    }
}
