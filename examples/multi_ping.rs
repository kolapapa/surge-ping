use std::net::IpAddr;
use std::time::Duration;

use surge_ping::{Client, Config, IcmpPacket, ICMP};
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ips = [
        "114.114.114.114",
        "8.8.8.8",
        "39.156.69.79",
        "172.217.26.142",
        "240c::6666",
        "2a02:930::ff76",
    ];
    let client_v4 = Client::new(&Config::default())?;
    let client_v6 = Client::new(&Config::builder().kind(ICMP::V6).build())?;
    let mut tasks = Vec::new();
    for ip in &ips {
        let addr: IpAddr = ip.parse()?;
        let surge = match addr {
            IpAddr::V4(_) => client_v4.clone(),
            IpAddr::V6(_) => client_v6.clone(),
        };
        tasks.push(tokio::spawn(async move {
            ping(surge, addr, 56).await.unwrap();
        }));
    }
    for t in tasks.into_iter() {
        t.await?;
    }
    //signal::ctrl_c().await?;
    //println!("ctrl-c received!");
    Ok(())
}
// Ping an address 5 times， and print output message（interval 1s）
async fn ping(client: Client, addr: IpAddr, size: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut pinger = client.pinger(addr).await;
    pinger.size(size).timeout(Duration::from_secs(1));
    let mut interval = time::interval(Duration::from_secs(1));
    for idx in 0..5 {
        interval.tick().await;
        match pinger.ping(idx).await {
            Ok((IcmpPacket::V4(packet), dur)) => println!(
                "No.{}: {} bytes from {}: icmp_seq={} ttl={} time={:0.2?}",
                idx,
                packet.get_size(),
                packet.get_source(),
                packet.get_sequence(),
                packet.get_ttl(),
                dur
            ),
            Ok((IcmpPacket::V6(packet), dur)) => println!(
                "No.{}: {} bytes from {}: icmp_seq={} hlim={} time={:0.2?}",
                idx,
                packet.get_size(),
                packet.get_source(),
                packet.get_sequence(),
                packet.get_max_hop_limit(),
                dur
            ),
            Err(e) => println!("No.{}: {} ping {}", idx, addr, e),
        };
    }
    println!("[+] {} done.", addr);
    Ok(())
}
