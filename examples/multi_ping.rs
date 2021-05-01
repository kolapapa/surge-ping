use std::net::IpAddr;
use std::time::Duration;

use surge_ping::Pinger;
use tokio::signal;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ips = [
        "114.114.114.114",
        "8.8.8.8",
        "39.156.69.79",
        "172.217.26.142",
    ];
    for ip in &ips {
        let addr: IpAddr = ip.parse()?;
        tokio::spawn(async move {
            ping(addr, 56).await.unwrap();
        });
    }
    signal::ctrl_c().await?;
    println!("ctrl-c received!");
    Ok(())
}
// Ping an address 5 times， and print output message（interval 1s）
async fn ping(addr: IpAddr, size: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut pinger = Pinger::new(addr)?;
    pinger.size(size).timeout(Duration::from_secs(1));
    let mut interval = time::interval(Duration::from_secs(1));
    for idx in 0..5 {
        interval.tick().await;
        match pinger.ping(idx).await {
            Ok((reply, dur)) => println!(
                "{} bytes from {}: icmp_seq={} ttl={} time={:?}",
                reply.size,
                reply.source,
                reply.sequence,
                match reply.ttl {
                    Some(ttl) => format!("{}", ttl),
                    None => "?".to_string(),
                },
                dur
            ),
            Err(e) => println!("{} ping {}", addr, e),
        };
    }
    println!("[+] {} done.", addr);
    Ok(())
}
