use std::time::Duration;

use structopt::StructOpt;
use surge_ping::{Client, Config, PingIdentifier, PingSequence, ICMP};

#[derive(StructOpt, Debug)]
#[structopt(name = "surge-ping")]
struct Opt {
    #[structopt(short = "h", long)]
    host: String,

    /// Specify the number of data bytes to be sent.  The default is 56, which translates into 64 ICMP
    /// data bytes when combined with the 8 bytes of ICMP header data.  This option cannot be used with
    /// ping sweeps.
    #[structopt(short = "s", long, default_value = "56")]
    size: usize,

    /// Source multicast packets with the given interface address.  This flag only applies if the ping
    /// destination is a multicast address.
    #[structopt(short = "I", long)]
    iface: Option<String>,

    #[structopt(short = "d", long)]
    dont_fragment: bool
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

    let mut config_builder = Config::builder();

    if let Some(interface) = opt.iface {
        config_builder = config_builder.interface(&interface);
    }

    config_builder = config_builder.dont_fragment(opt.dont_fragment);

    if ip.is_ipv6() {
        config_builder = config_builder.kind(ICMP::V6);
    }
    let config = config_builder.build();

    let payload = vec![0; opt.size];
    let client = Client::new(&config).unwrap();
    let mut pinger = client.pinger(ip, PingIdentifier(111)).await;
    pinger.timeout(Duration::from_secs(1));
    match pinger.ping(PingSequence(0), &payload).await {
        Ok((packet, rtt)) => {
            println!("{:?} {:0.2?}", packet, rtt);
        }
        Err(e) => println!("{}", e),
    };
}
