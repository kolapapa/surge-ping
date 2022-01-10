use std::time::Duration;

use structopt::StructOpt;
use surge_ping::{Client, Config};

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

    let config = match opt.iface {
        Some(val) => Config::builder().interface(&val).build(),
        None => Config::default(),
    };

    let client = Client::new(&config).unwrap();
    let mut pinger = client.pinger(ip).await;
    pinger
        .ident(111)
        .size(opt.size)
        .timeout(Duration::from_secs(1));
    match pinger.ping(0).await {
        Ok((packet, rtt)) => {
            println!("{:?} {:0.2?}", packet, rtt);
        }
        Err(e) => println!("{}", e),
    };
}
