use std::{
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

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
/// use surge::Pinger;
///
/// #[tokio::main]
/// async fn main() {
///     let mut pinger = Pinger::new("114.114.114.114".parse().unwrap()).unwrap();
///     pinger.ident(123).size(56).timeout(Duration::from_secs(1));
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

    pub fn ident(&mut self, val: u16) -> &mut Self {
        self.ident = val;
        self
    }

    pub fn size(&mut self, size: usize) -> &mut Self {
        self.size = size;
        self
    }

    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = timeout;
        self
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

        let mut buffer = [0; 2048];

        let answer;
        tokio::select! {
            size = self.socket.recv(&mut buffer) => {
                let echo_reply = EchoReply::decode(&buffer[..size?])?;
                answer = Ok((echo_reply, Instant::now() - send_time));
            }
            _ = sleep(self.timeout) => answer = Err(SurgeError::Timeout),
        };
        answer
    }
}
