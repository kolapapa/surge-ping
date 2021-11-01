use std::sync::Arc;
use std::{io, net::IpAddr};

use crate::ping::Pinger;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::Mutex;

#[cfg(unix)]
use std::os::unix::io::{FromRawFd, IntoRawFd};
#[cfg(windows)]
use std::os::windows::io::{FromRawSocket, IntoRawSocket};

pub struct PingSocketBuilder {
    socket: Socket,
}
impl PingSocketBuilder {
    pub fn new(d: Domain) -> io::Result<PingSocketBuilder> {
        let socket = match d {
            Domain::IPV4 => Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?,
            Domain::IPV6 => Socket::new(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))?,
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid domain",
                ))
            }
        };

        // TODO: Type filtering,
        // https://tools.ietf.org/html/rfc3542#section-3.2. Currently blocked
        // on https://github.com/rust-lang/socket2/issues/199

        // TODO: Get access to the hop limits
        // https://tools.ietf.org/html/rfc3542#section-4, to show the TTL for
        // ICMPv6.
        socket.set_nonblocking(true)?;
        Ok(PingSocketBuilder { socket })
    }
    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    pub fn bind_device(&self, interface: Option<&[u8]>) -> io::Result<()> {
        self.socket.bind_device(interface)
    }

    #[cfg(all(feature = "all", any(target_os = "freebsd")))]
    pub fn set_fib(&self, fib:u32) -> io::Result<()> {
        self.socket.set_fib(fib)
    }

    pub fn bind_addr(&self, sock_addr: &SockAddr) -> io::Result<()> {
        self.socket.bind(sock_addr)
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.socket.set_ttl(ttl)
    }

    pub fn set_send_buffer_size(&self, bufsize: usize) -> io::Result<()> {
        self.socket.set_send_buffer_size(bufsize)
    }

    pub fn set_recv_buffer_size(&self, bufsize: usize) -> io::Result<()> {
        self.socket.set_recv_buffer_size(bufsize)
    }
    fn inner_run(self) -> io::Result<UdpSocket> {
        #[cfg(windows)]
        return Ok(UdpSocket::from_std(unsafe {
            std::net::UdpSocket::from_raw_socket(self.socket.into_raw_socket())
        })?);
        #[cfg(unix)]
        return Ok(UdpSocket::from_std(unsafe {
            std::net::UdpSocket::from_raw_fd(self.socket.into_raw_fd())
        })?);
    }

    pub fn run(self) -> io::Result<PingSocket> {
        PingSocket::new_socket(AsyncSocket::new(self.inner_run()?))
    }
}

#[derive(Clone)]
pub(crate) struct AsyncSocket {
    inner: Arc<UdpSocket>,
}
impl AsyncSocket {
    fn new(socket: UdpSocket) -> Self {
        AsyncSocket {
            inner: Arc::new(socket),
        }
    }
    pub async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }
    pub async fn send_to(&self, buf: &mut [u8], target: &SocketAddr) -> io::Result<usize> {
        self.inner.send_to(buf, target).await
    }
}
#[derive(Clone)]
pub struct PingSocket {
    inner: AsyncSocket,
    pmap: Arc<Mutex<BTreeMap<IpAddr, Sender<Vec<u8>>>>>,
    recv_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl PingSocket {
    pub fn new(d: Domain) -> io::Result<PingSocket> {
        PingSocketBuilder::new(d)?.run()
    }
    fn new_socket(inner: AsyncSocket) -> io::Result<PingSocket> {
        Ok(PingSocket {
            inner: inner,
            pmap: Arc::new(Mutex::new(BTreeMap::new())),
            recv_task: Arc::new(Mutex::new(None)),
        })
    }
    pub(crate) fn create_pinger(addr: IpAddr) -> io::Result<Pinger> {
        let domain = match addr {
            IpAddr::V4(_) => socket2::Domain::IPV4,
            IpAddr::V6(_) => socket2::Domain::IPV6,
        };
        let inner = AsyncSocket::new(PingSocketBuilder::new(domain)?.inner_run()?);
        let mut pmap = BTreeMap::<IpAddr, Sender<Vec<u8>>>::new();
        let recv_task = Arc::new(Mutex::new(None));
        let (tx, rx) = channel(100);
        pmap.insert(addr.clone(), tx);
        let pmap = Arc::new(Mutex::new(pmap));
        Self::run_task(inner.clone(), pmap.clone(), recv_task.clone());
        Ok(Pinger::new_pinger(addr, inner.clone(), rx))
    }
    fn run_task(
        inner: AsyncSocket,
        pmap: Arc<Mutex<BTreeMap<IpAddr, Sender<Vec<u8>>>>>,
        recv_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::task::spawn(async move {
            let mut buffer = [0 as u8; 2048];
            loop {
                let (sz, from_addr) = match inner.recv_from(&mut buffer).await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let mut pmapguard = pmap.lock().await;
                let tx = match pmapguard.get(&from_addr.ip()) {
                    None => continue,
                    Some(tx) => tx,
                };
                //let btosend = unsafe { assume_init(&buffer[0..sz]) }.to_vec();
                if tx.try_send(buffer[0..sz].to_vec()).is_err() {
                    pmapguard.remove(&from_addr.ip());
                    if pmapguard.len() < 1 {
                        break;
                    }
                };
            }
            let mut guard_task = recv_task.lock().await;
            *guard_task = None;
        })
    }
    async fn check_task(&self) {
        let mut guard_task = self.recv_task.lock().await;
        if guard_task.is_some() {
            return;
        }
        *guard_task = Some(Self::run_task(
            self.inner.clone(),
            self.pmap.clone(),
            self.recv_task.clone(),
        ));
    }
    pub async fn pinger(&self, addr: IpAddr) -> Pinger {
        let (tx, rx) = channel(100);
        self.pmap.lock().await.insert(addr.clone(), tx);
        self.check_task().await;
        Pinger::new_pinger(addr, self.inner.clone(), rx)
    }
}
