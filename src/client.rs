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

use log::trace;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::{
    net::UdpSocket,
    sync::{broadcast, mpsc, Mutex},
    task,
};

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
    mapping: Arc<Mutex<HashMap<IpAddr, mpsc::Sender<Message>>>>,
}

impl Client {
    /// A client is generated according to the configuration. In fact, a `AsyncSocket` is wrapped inside,
    /// and you can clone to any `task` at will.
    pub fn new(config: &Config) -> io::Result<Self> {
        let socket = AsyncSocket::new(config)?;
        Ok(Self {
            socket,
            mapping: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Create a `Pinger` instance, you can make special configuration for this instance. Such as `timeout`, `size` etc.
    pub async fn pinger(&self, host: IpAddr) -> Pinger {
        let (shutdown_tx, _) = broadcast::channel(1);
        let (tx, rx) = mpsc::channel(10);
        {
            self.mapping.lock().await.insert(host, tx);
        }
        task::spawn(recv_task(
            self.socket.clone(),
            self.mapping.clone(),
            shutdown_tx.subscribe(),
        ));
        Pinger::new(host, self.socket.clone(), rx, shutdown_tx)
    }
}

async fn recv_task(
    socket: AsyncSocket,
    mapping: Arc<Mutex<HashMap<IpAddr, mpsc::Sender<Message>>>>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut buf = [0; 1024];
    loop {
        tokio::select! {
            answer = socket.recv_from(&mut buf) => {
                if let Ok((sz, addr)) = answer {
                    let instant = Instant::now();
                    let mut w = mapping.lock().await;
                    if let Some(tx) = (*w).get(&addr.ip()) {
                        if tx.send(Message::new(instant, buf[0..sz].to_vec())).await.is_err() {
                            trace!("send message error");
                            (*w).remove(&addr.ip());
                        }
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                break
            }
        }
    }
}
