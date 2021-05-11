use std::{net::IpAddr, time::Duration};

use pnet_packet::icmp::IcmpTypes;
use rand::random;
use structopt::StructOpt;
use surge_ping::Pinger;

#[derive(StructOpt, Debug)]
#[structopt(name = "surge-ping")]
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

    #[structopt(short = "m", long, default_value = "64")]
    max_hops: u32,
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

    println!(
        "traceroute to {} ({}), {} hops max, {} byte packets",
        opt.host, ip, opt.max_hops, opt.size
    );
    let mut pinger = Pinger::new(ip).unwrap();
    pinger
        .ident(random())
        .size(opt.size)
        .timeout(Duration::from_secs(1));
    let mut stop_flag = false;
    let mut durations: Vec<Option<Duration>> = vec![None; 3];
    for ttl in 1..=opt.max_hops {
        let _ = pinger.set_ttl(ttl);
        let mut hop_addr: Option<IpAddr> = None;
        for item in &mut durations {
            match pinger.ping(ttl as u16).await {
                Ok(answer) => {
                    if answer.0.icmp_type == IcmpTypes::EchoReply {
                        stop_flag = true;
                    }
                    hop_addr = Some(answer.0.source);
                    *item = Some(answer.1);
                }
                Err(_) => *item = None,
            }
        }
        match hop_addr {
            Some(addr) => {
                println!(
                    "{} {} ({}) {}",
                    ttl,
                    addr,
                    addr,
                    durations2string(&durations)
                );
            }
            None => {
                println!("{} * * *", ttl,);
            }
        }

        if stop_flag {
            break;
        }
    }
}

fn durations2string(durs: &[Option<Duration>]) -> String {
    let dur_strings = durs
        .iter()
        .map(|val| {
            val.map(|v| format!("{:.5}ms", v.as_millis()))
                .unwrap_or_else(|| "*".to_owned())
        })
        .collect::<Vec<String>>();
    dur_strings.join(" ")
}
