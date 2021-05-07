use std::{
    collections::HashMap,
    mem::MaybeUninit,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

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

/// A Ping struct represents the state of one particular ping instance.
///
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
    /// Creates a new Ping instance from `IpAddr`.
    pub fn new(host: IpAddr) -> Result<Pinger> {
        Ok(Pinger {
            host,
            ident: random(),
            size: 56,
            timeout: Duration::from_secs(2),
            socket: AsyncSocket::new(host)?,
            cache: Cache::new(),
        })
    }

    /// Sets the value for the `SO_BINDTODEVICE` option on this socket.
    ///
    /// If a socket is bound to an interface, only packets received from that
    /// particular interface are processed by the socket. Note that this only
    /// works for some socket types, particularly `AF_INET` sockets.
    ///
    /// If `interface` is `None` or an empty string it removes the binding.
    ///
    /// This function is only available on Fuchsia and Linux.
    #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
    pub fn bind_device(&mut self, interface: Option<&[u8]>) -> Result<&mut Pinger> {
        self.socket.bind_device(interface)?;
        Ok(self)
    }

    /// Set the identification of ICMP.
    pub fn ident(&mut self, val: u16) -> &mut Pinger {
        self.ident = val;
        self
    }

    /// Set the packet size.(default: 56)
    pub fn size(&mut self, size: usize) -> &mut Pinger {
        self.size = size;
        self
    }

    /// The timeout of each Ping, in seconds. (default: 2s)
    pub fn timeout(&mut self, timeout: Duration) -> &mut Pinger {
        self.timeout = timeout;
        self
    }

    async fn recv_reply(&self, seq_cnt: u16) -> Result<(EchoReply, Duration)> {
        let mut buffer = [MaybeUninit::new(0); 2048];
        loop {
            let size = self.socket.recv(&mut buffer).await?;
            let buf = unsafe { assume_init(&buffer[..size]) };
            match EchoReply::decode(self.host, buf) {
                Ok(reply) => {
                    // check reply ident is same
                    if reply.identifier == self.ident && reply.sequence == seq_cnt {
                        if let Some(ins) = self.cache.remove(self.ident, seq_cnt) {
                            return Ok((reply, Instant::now() - ins));
                        }
                    }
                    continue;
                }
                Err(SurgeError::NotEchoReply(_)) => continue,
                Err(SurgeError::NotV6EchoReply(_)) => continue,
                Err(SurgeError::OtherICMP) => continue,
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }

    /// Send Ping request with sequence number.
    pub async fn ping(&self, seq_cnt: u16) -> Result<(EchoReply, Duration)> {
        let sender = self.socket.clone();
        let mut packet = EchoRequest::new(self.host, self.ident, seq_cnt, self.size).encode()?;
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

/// Assume the `buf`fer to be initialised.
// TODO: replace with `MaybeUninit::slice_assume_init_ref` once stable.
unsafe fn assume_init(buf: &[MaybeUninit<u8>]) -> &[u8] {
    &*(buf as *const [MaybeUninit<u8>] as *const [u8])
}
