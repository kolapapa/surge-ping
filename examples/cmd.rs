use std::time::Duration;

use structopt::StructOpt;
use surge_ping::Pinger;
use tokio::time;

#[derive(Default, Debug)]
struct Answer {
    host: String,
    transmitted: usize,
    received: usize,
    durations: Vec<Duration>,
}
impl Answer {
    fn new(host: &str) -> Answer {
        Answer {
            host: host.to_owned(),
            transmitted: 0,
            received: 0,
            durations: Vec::new(),
        }
    }

    fn update(&mut self, dur: Option<Duration>) {
        match dur {
            Some(dur) => {
                self.transmitted += 1;
                self.received += 1;
                self.durations.push(dur);
            }
            None => self.transmitted += 1,
        }
    }

    fn min(&self) -> Option<f64> {
        let min = self
            .durations
            .iter()
            .min()
            .map(|dur| dur.as_secs_f64() * 1000f64);
        min
    }

    fn max(&self) -> Option<f64> {
        let max = self
            .durations
            .iter()
            .max()
            .map(|dur| dur.as_secs_f64() * 1000f64);
        max
    }

    fn avg(&self) -> Option<f64> {
        let sum: Duration = self.durations.iter().sum();
        let avg = sum
            .checked_div(self.durations.iter().len() as u32)
            .map(|dur| dur.as_secs_f64() * 1000f64);
        avg
    }

    fn mdev(&self) -> Option<f64> {
        if let Some(avg) = self.avg() {
            let tmp_sum = self.durations.iter().fold(0_f64, |acc, x| {
                acc + x.as_secs_f64() * x.as_secs_f64() * 1000000f64
            });
            let tmdev = tmp_sum / self.durations.iter().len() as f64 - avg * avg;
            Some(tmdev.sqrt())
        } else {
            None
        }
    }

    fn output(&self) {
        println!("\n--- {} ping statistics ---", self.host);
        println!(
            "{} packets transmitted, {} packets received, {:.2}% packet loss",
            self.transmitted,
            self.received,
            (self.transmitted - self.received) as f64 / self.transmitted as f64 * 100_f64
        );
        if self.received > 1 {
            println!(
                "round-trip min/avg/max/stddev = {:.3}/{:.3}/{:.3}/{:.3} ms",
                self.min().unwrap(),
                self.avg().unwrap(),
                self.max().unwrap(),
                self.mdev().unwrap()
            );
        }
    }
}

#[derive(StructOpt, Debug)]
#[structopt(name = "surge-ping")]
struct Opt {
    #[structopt(short = "h", long)]
    host: String,

    /// Wait wait milliseconds between sending each packet.  The default is to wait for one second between
    /// each packet.
    #[structopt(short = "i", long, default_value = "1.0")]
    interval: f64,

    /// Specify the number of data bytes to be sent.  The default is 56, which translates into 64 ICMP
    /// data bytes when combined with the 8 bytes of ICMP header data.  This option cannot be used with
    /// ping sweeps.
    #[structopt(short = "s", long, default_value = "56")]
    size: usize,

    /// Stop after sending (and receiving) count ECHO_RESPONSE packets.
    /// If this option is not specified, ping will operate until interrupted.
    /// If this option is specified in conjunction with ping sweeps, each
    /// sweep will consist of count packets.
    #[structopt(short = "c", long, default_value = "5")]
    count: u16,

    /// Source multicast packets with the given interface address.  This flag only applies if the ping
    /// destination is a multicast address.
    #[structopt(short = "I", long)]
    iface: Option<String>,

    /// Specify a timeout, in seconds, before ping exits regardless of
    /// how many packets have been received.
    #[structopt(short = "t", long, default_value = "1")]
    timeout: u64,
}

#[tokio::main]
async fn main() {
    let opt = Opt::from_args();

    let ip = tokio::net::lookup_host(format!("{}:0", opt.host))
        .await
        .expect("host lookup error")
        .next()
        .map(|val| val.ip())
        .unwrap();

    let mut interval = time::interval(Duration::from_millis((opt.interval * 1000f64) as u64));
    let mut pinger = Pinger::new(ip).unwrap();
    pinger.timeout(Duration::from_secs(opt.timeout));

    #[cfg(target_os = "linux")]
    pinger
        .bind_device(opt.iface.as_deref().map(|val| val.as_bytes()))
        .unwrap();

    let mut answer = Answer::new(&opt.host);
    println!("PING {} ({}): {} data bytes", opt.host, ip, opt.size);
    for idx in 0..opt.count {
        interval.tick().await;
        match pinger.ping(idx).await {
            Ok((reply, dur)) => {
                println!(
                    "{} bytes from {}: icmp_seq={} ttl={} time={:.3} ms",
                    reply.size,
                    reply.source,
                    reply.sequence,
                    match reply.ttl {
                        Some(ttl) => format!("{}", ttl),
                        None => "?".to_string(),
                    },
                    dur.as_secs_f64() * 1000f64
                );
                answer.update(Some(dur));
            }
            Err(e) => {
                println!("{}", e);
                answer.update(None);
            }
        }
    }
    answer.output();
}
