use std::mem::MaybeUninit;
use std::sync::Arc;
use std::{io, net::IpAddr};

use socket2::{Domain, SockAddr, Socket};
use std::collections::BTreeMap;
use tokio::io::unix::AsyncFd;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::Mutex;

use crate::ping::Pinger;
use crate::unix::AsyncSocket;

#[derive(Debug, Clone)]
pub struct PingSocket {
    inner: AsyncSocket,
    pmap: Arc<Mutex<BTreeMap<IpAddr, Sender<Vec<u8>>>>>,
    recv_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl PingSocket {
    pub fn new(d: Domain) -> io::Result<PingSocket> {
        Ok(PingSocket {
            inner: AsyncSocket::new(d)?,
            pmap: Arc::new(Mutex::new(BTreeMap::new())),
            recv_task: Arc::new(Mutex::new(None)),
        })
    }
    pub(crate) fn create_pinger(addr: IpAddr) -> io::Result<Pinger> {
        let domain = match addr {
            IpAddr::V4(_) => socket2::Domain::IPV4,
            IpAddr::V6(_) => socket2::Domain::IPV6,
        };
        let inner = AsyncSocket::new(domain)?;
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
            let mut buffer = [MaybeUninit::new(0); 2048];
            loop {
                let (sz, from) = match inner.recv_from(&mut buffer).await {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let from_addr = match from.as_socket_ipv4() {
                    Some(v4) => IpAddr::V4(*v4.ip()),
                    None => match from.as_socket_ipv6() {
                        Some(v6) => IpAddr::V6(*v6.ip()),
                        None => continue,
                    },
                };
                let mut pmapguard = pmap.lock().await;
                let tx = match pmapguard.get(&from_addr) {
                    None => continue,
                    Some(tx) => tx,
                };
                let btosend = unsafe { assume_init(&buffer[0..sz]) }.to_vec();
                if tx.try_send(btosend).is_err() {
                    pmapguard.remove(&from_addr);
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

    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    pub fn bind_device(&self, interface: Option<&[u8]>) -> io::Result<()> {
        self.inner.bind_device(interface)
    }

    pub fn bind_addr(&self, sock_addr: &SockAddr) -> io::Result<()> {
        self.inner.bind_addr(sock_addr)
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.inner.set_ttl(ttl)
    }

    pub fn set_send_buffer_size(&self, bufsize: usize) -> io::Result<()> {
        self.inner.set_send_buffer_size(bufsize)
    }

    pub fn set_recv_buffer_size(&self, bufsize: usize) -> io::Result<()> {
        self.inner.set_recv_buffer_size(bufsize)
    }

    pub async fn send_to(
        socket: Arc<AsyncFd<Socket>>,
        buf: &mut [u8],
        target: &SockAddr,
    ) -> io::Result<usize> {
        loop {
            let mut guard = socket.writable().await?;

            match guard.try_io(|inner| inner.get_ref().send_to(buf, target)) {
                Ok(n) => return n,
                Err(_would_block) => continue,
            }
        }
    }
}

/// Assume the `buf`fer to be initialised.
// TODO: replace with `MaybeUninit::slice_assume_init_ref` once stable.
unsafe fn assume_init(buf: &[MaybeUninit<u8>]) -> &[u8] {
    &*(buf as *const [MaybeUninit<u8>] as *const [u8])
}
