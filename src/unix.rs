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
    pub fn new() -> io::Result<AsyncSocket> {
        let socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
        socket.set_nonblocking(true)?;
        Ok(AsyncSocket {
            inner: Arc::new(AsyncFd::new(socket)?),
        })
    }

    #[cfg(target_os = "linux")]
    pub fn bind_device(&self, interface: Option<&[u8]>) -> io::Result<()> {
        self.inner.get_ref().bind_device(interface)
    }

    pub async fn recv(&self, buf: &mut [MaybeUninit<u8>]) -> io::Result<usize> {
        loop {
            let mut guard = self.inner.readable().await?;

            match guard.try_io(|inner| inner.get_ref().recv(buf)) {
                Ok(result) => return result,
                Err(_would_block) => continue,
            }
        }
    }

    pub async fn send_to(&self, buf: &mut [u8], target: &SockAddr) -> io::Result<usize> {
        loop {
            let mut guard = self.inner.writable().await?;

            match guard.try_io(|inner| inner.get_ref().send_to(buf, target)) {
                Ok(n) => return n,
                Err(_would_block) => continue,
            }
        }
    }
}
