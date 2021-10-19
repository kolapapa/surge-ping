use std::io;
use std::mem::MaybeUninit;
use std::sync::Arc;

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::io::unix::AsyncFd;

#[derive(Debug, Clone)]
pub struct AsyncSocket {
    inner: Arc<AsyncFd<Socket>>,
}

impl AsyncSocket {
    pub fn new(d: Domain) -> io::Result<AsyncSocket> {
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
        Ok(AsyncSocket {
            inner: Arc::new(AsyncFd::new(socket)?),
        })
    }

    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    pub fn bind_device(&self, interface: Option<&[u8]>) -> io::Result<()> {
        self.inner.get_ref().bind_device(interface)
    }

    pub fn bind_addr(&self, sock_addr: &SockAddr) -> io::Result<()> {
        self.inner.get_ref().bind(sock_addr)
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.inner.get_ref().set_ttl(ttl)
    }

    pub fn set_send_buffer_size(&self, bufsize: usize) -> io::Result<()> {
        self.inner.get_ref().set_send_buffer_size(bufsize)
    }

    pub fn set_recv_buffer_size(&self, bufsize: usize) -> io::Result<()> {
        self.inner.get_ref().set_recv_buffer_size(bufsize)
    }

    pub async fn recv_from(&self, buf: &mut [MaybeUninit<u8>]) -> io::Result<(usize, SockAddr)> {
        loop {
            let mut guard = self.inner.readable().await?;

            match guard.try_io(|inner| inner.get_ref().recv_from(buf)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }
    pub async fn send_to(&self, buf: &mut [u8], target: &SockAddr) -> io::Result<usize> {
        let socket = self.inner.clone();
        loop {
            let mut guard = socket.writable().await?;

            match guard.try_io(|inner| inner.get_ref().send_to(buf, target)) {
                Ok(n) => return n,
                Err(_would_block) => continue,
            }
        }
    }
}
