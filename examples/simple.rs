#[macro_use]
extern crate log;
extern crate pretty_env_logger;

use std::time::Duration;

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

    let mut pinger = Pinger::new(ip).unwrap();
    pinger
        .ident(111)
        .size(opt.size)
        .timeout(Duration::from_secs(1));
    let answer = pinger.ping(0).await;
    info!("{:?}", answer);
}
