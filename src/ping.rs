#[cfg(target_os = "linux")]
use std::ffi::CStr;
use std::{
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

use packet::icmp::Kind;
use rand::random;
use tokio::task;
use tokio::time::sleep;

use crate::error::{Result, SurgeError};
use crate::icmp::{EchoReply, EchoRequest};
use crate::unix::AsyncSocket;

/// # Examples
/// ```
/// use std::time::Duration;
///
/// use surge_ping::Pinger;
///
/// #[tokio::main]
/// async fn main() {
///     let mut pinger = Pinger::new("114.114.114.114".parse().unwrap()).unwrap();
///     pinger.size(56).timeout(Duration::from_secs(1));
///     let result = pinger.ping(0).await;
///     println!("{:?}", result);
/// }
///
#[derive(Debug, Clone)]
pub struct Pinger {
    host: IpAddr,
    ident: u16,
    size: usize,
    timeout: Duration,
    socket: AsyncSocket,
}

impl Pinger {
    pub fn new(host: IpAddr) -> Result<Pinger> {
        Ok(Pinger {
            host,
            ident: random(),
            size: 56,
            timeout: Duration::from_secs(2),
            socket: AsyncSocket::new()?,
        })
    }

    #[cfg(target_os = "linux")]
    pub fn bind_device(&mut self, interface: Option<&CStr>) -> Result<&mut Pinger> {
        self.socket.bind_device(interface)?;
        Ok(self)
    }

    pub fn ident(&mut self, val: u16) -> &mut Pinger {
        self.ident = val;
        self
    }

    pub fn size(&mut self, size: usize) -> &mut Pinger {
        self.size = size;
        self
    }

    pub fn timeout(&mut self, timeout: Duration) -> &mut Pinger {
        self.timeout = timeout;
        self
    }

    async fn recv_reply(&self) -> Result<EchoReply> {
        let mut buffer = [0; 2048];
        loop {
            let size = self.socket.recv(&mut buffer).await?;
            match EchoReply::decode(&buffer[..size]) {
                Ok(reply) => {
                    // check reply ident is same
                    if reply.identifier == self.ident {
                        return Ok(reply);
                    }
                }
                Err(SurgeError::KindError(Kind::EchoRequest)) => {
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }

    pub async fn ping(&self, seq_cnt: u16) -> Result<(EchoReply, Duration)> {
        let sender = self.socket.clone();
        let mut packet = EchoRequest::new(self.ident, seq_cnt, self.size).encode()?;
        let sock_addr = SocketAddr::new(self.host, 0);
        let send_time = Instant::now();
        task::spawn(async move {
            let _size = sender
                .send_to(&mut packet, &sock_addr.into())
                .await
                .expect("socket send packet error");
        });

        tokio::select! {
            reply = self.recv_reply() => {
                reply.map(|echo_reply| (echo_reply, Instant::now() - send_time))
            }
            _ = sleep(self.timeout) => Err(SurgeError::Timeout),
        }
    }
}
