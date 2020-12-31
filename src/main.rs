mod icmp;
mod unix;

use crate::unix::AsyncSocket;
use std::time::Instant;
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use once_cell::sync::Lazy;
use parking_lot::Mutex;

type Token = [u8; 8];

static CACHE: Lazy<Mutex<HashMap<Token, Instant>>> = Lazy::new(|| Mutex::new(HashMap::new()));

async fn send(to: IpAddr, size: usize, socket: AsyncSocket) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        let mut seq_cnt = 0;
        loop {
            interval.tick().await;
            println!("No.{} ({}bytes)", seq_cnt, size);
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
                Err(e) => println!("No.{} send error: {}", seq_cnt, e),
            };
            seq_cnt += 1;
        }
    });
}

async fn recv_loop(socket: AsyncSocket) {
    loop {
        let mut buffer = [0; 2048];
        let size = socket.recv(&mut buffer).await.unwrap();
        let answer = icmp::parse_token(&buffer[20..size]);
        let recv_time = Instant::now();
        match answer {
            Ok((seq_cnt, token)) => {
                let mut w = CACHE.lock();
                let send_time = (*w).remove(&token);
                if let Some(send_time) = send_time {
                    let dur = recv_time - send_time;
                    println!("No.{} rta: {:?}", seq_cnt, dur);
                }
            }
            Err(e) => println!("{:?}", e),
        }
    }
}

#[tokio::main]
async fn main() {
    let socket = AsyncSocket::new(None).expect("socket create error");

    let to: IpAddr = "114.114.114.114".parse().unwrap();
    send(to, 56, socket.clone()).await;
    recv_loop(socket.clone()).await;
}
