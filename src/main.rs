#[macro_use]
extern crate log;
extern crate pretty_env_logger;

use std::time::Instant;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use structopt::StructOpt;

mod icmp;
mod unix;

use crate::icmp::Token;
use crate::unix::AsyncSocket;

static CACHE: Lazy<Mutex<HashMap<Token, Instant>>> = Lazy::new(|| Mutex::new(HashMap::new()));

async fn send(to: IpAddr, size: usize, interval: u64, socket: AsyncSocket) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(interval));
        let mut seq_cnt = 0;
        loop {
            interval.tick().await;
            // info!("No.{} ({}bytes)", seq_cnt, size);
            let (mut packet, token) = icmp::make_echo_request(111, seq_cnt, size).unwrap();
            let send_time = Instant::now();
            let res = socket
                .send_to(&mut packet, &SocketAddr::new(to, 0).into())
                .await;
            match res {
                Ok(_) => {
                    let mut m = CACHE.lock();
                    (*m).insert(token, send_time);
                }
                Err(e) => error!("No.{} send error: {}", seq_cnt, e),
            };
            seq_cnt += 1;
        }
    });
}

async fn recv_loop(socket: AsyncSocket) {
    loop {
        let mut buffer = [0; 2048];
        let size = socket.recv(&mut buffer).await.unwrap();
        let echo_reply = icmp::EchoReply::decode(&buffer[..size]);
        let recv_time = Instant::now();
        match echo_reply {
            Ok(reply) => {
                let mut w = CACHE.lock();
                let send_time = (*w).remove(&reply.token);
                if let Some(send_time) = send_time {
                    let dur = recv_time - send_time;
                    info!(
                        "{} bytes from {}: icmp_seq={} ttl={} time={:?}",
                        reply.size, reply.source, reply.sequence, reply.ttl, dur
                    );
                }
            }
            Err(e) => error!("{:?}", e),
        }
    }
}

#[derive(StructOpt, Debug)]
#[structopt(name = "surge")]
struct Opt {
    #[structopt(short = "h", long)]
    host: String,

    /// Wait wait milliseconds between sending each packet.  The default is to wait for one second between
    /// each packet.
    #[structopt(short = "i", long, default_value = "1000")]
    interval: u64,

    /// Specify the number of data bytes to be sent.  The default is 56, which translates into 64 ICMP
    /// data bytes when combined with the 8 bytes of ICMP header data.  This option cannot be used with
    /// ping sweeps.
    #[structopt(short = "s", long, default_value = "56")]
    size: usize,

    /// Stop after sending (and receiving) count ECHO_RESPONSE packets.
    /// If this option is not specified, ping will operate until interrupted.
    /// If this option is specified in conjunction with ping sweeps, each
    /// sweep will consist of count packets.
    #[structopt(short = "c", long)]
    count: Option<u64>,

    /// Source multicast packets with the given interface address.  This flag only applies if the ping
    /// destination is a multicast address.
    #[structopt(short = "I", long)]
    iface: Option<String>,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let opt = Opt::from_args();

    let ip = tokio::net::lookup_host(format!("{}:0", opt.host))
        .await
        .expect("host lookup error")
        .next()
        .map(|val| val.ip())
        .unwrap();
    info!("PING {} ({}): {} data bytes", opt.host, ip, opt.size);
    let socket = AsyncSocket::new().expect("socket create error");

    send(ip, opt.size, opt.interval, socket.clone()).await;
    recv_loop(socket.clone()).await;
}
