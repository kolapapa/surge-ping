#[cfg(target_os = "linux")]
use std::ffi::CStr;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use packet::icmp::Kind;
use parking_lot::Mutex;
use rand::random;
use tokio::task;
use tokio::time::sleep;

use crate::error::{Result, SurgeError};
use crate::icmp::{EchoReply, EchoRequest};
use crate::unix::AsyncSocket;

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
    cache: Cache,
}

impl Pinger {
    pub fn new(host: IpAddr) -> Result<Pinger> {
        Ok(Pinger {
            host,
            ident: random(),
            size: 56,
            timeout: Duration::from_secs(2),
            socket: AsyncSocket::new()?,
            cache: Cache::new(),
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

    async fn recv_reply(&self, seq_cnt: u16) -> Result<(EchoReply, Duration)> {
        let mut buffer = [0; 2048];
        loop {
            let size = self.socket.recv(&mut buffer).await?;
            match EchoReply::decode(self.host, &buffer[..size]) {
                Ok(reply) => {
                    // check reply ident is same
                    if reply.identifier == self.ident && reply.sequence == seq_cnt {
                        if let Some(ins) = self.cache.remove(self.ident, seq_cnt) {
                            return Ok((reply, Instant::now() - ins));
                        }
                    }
                    continue;
                }
                Err(SurgeError::KindError(Kind::EchoRequest)) => continue,
                Err(SurgeError::OtherICMP) => continue,
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
        let ident = self.ident;
        let cache = self.cache.clone();
        task::spawn(async move {
            let _size = sender
                .send_to(&mut packet, &sock_addr.into())
                .await
                .expect("socket send packet error");
            cache.insert(ident, seq_cnt, Instant::now());
        });

        tokio::select! {
            reply = self.recv_reply(seq_cnt) => {
                reply.map_err(|err| {
                    self.cache.remove(ident, seq_cnt);
                    err
                })
            },
            _ = sleep(self.timeout) => {
                self.cache.remove(ident, seq_cnt);
                Err(SurgeError::Timeout)
            },
        }
    }
}
