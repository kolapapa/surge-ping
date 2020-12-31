use std::io;
use std::{ffi::CStr, sync::Arc};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::io::unix::AsyncFd;

#[derive(Clone)]
pub struct AsyncSocket {
    inner: Arc<AsyncFd<Socket>>,
}

impl AsyncSocket {
    pub fn new(interface: Option<&CStr>) -> io::Result<AsyncSocket> {
        let socket = Socket::new(Domain::ipv4(), Type::raw(), Some(Protocol::icmpv4()))?;
        #[cfg(target_os = "linux")]
        socket.bind_device(interface)?;
        socket.set_nonblocking(true)?;
        Ok(AsyncSocket {
            inner: Arc::new(AsyncFd::new(socket)?),
        })
    }

    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
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
