# surge-ping
[![Crates.io](https://img.shields.io/crates/v/surge-ping.svg)](https://crates.io/crates/surge-ping)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kolapapa/surge-ping/blob/main/LICENSE)
[![API docs](https://docs.rs/surge-ping/badge.svg)](http://docs.rs/surge-ping)

rust ping libray based on `tokio` + `socket2` + `pnet_packet`.

### Care
- `IPv6` is not fully implemented. If you have a need for `IPv6`, you can submit a `PR` and build together.
- Does not support Windows, later support, welcome to submit PR.


### Example
```rust
use std::time::Duration;

use surge_ping::Pinger;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut pinger = Pinger::new("114.114.114.114".parse()?)?;
    pinger.timeout(Duration::from_secs(1));
    for seq_cnt in 0..10 {
        let (reply, dur) = pinger.ping(seq_cnt).await?;
        println!(
            "{} bytes from {}: icmp_seq={} ttl={:?} time={:?}",
            reply.size, reply.source, reply.sequence, reply.ttl, dur
        );
    }
    Ok(())
}

```

You can send ICMP packets with custom interface or `set_ttl`
```rust
pinger.bind_device(Some("eth0".as_bytes()))?;

# You can rely on ttl to implement the icmp version of the traceroute program.

pinger.set_ttl(20)?;
```


### Ping(ICMP)
There are two example programs that you can run on your own.
```shell
$ git clone https://github.com/kolapapa/surge-ping.git
$ cd surge-ping


$ cargo build --example simple
sudo RUST_LOG=info ./target/debug/examples/simple -h www.baidu.com -s 56
INFO  simple > Ok((Icmpv4(Icmpv4Packet { source: 110.242.68.4, destination: 10.1.33.227, ttl: 45, icmp_type: IcmpType(0), icmp_code: IcmpCode(0), size: 64, identifier: 111, sequence: 0 }), 14.687909ms))

$ cargo build --example cmd
sudo ./target/debug/examples/cmd -h www.baidu.com -c 5
PING www.baidu.com (110.242.68.4): 56 data bytes
64 bytes from 110.242.68.4: icmp_seq=0 ttl=45 time=12.721 ms
64 bytes from 110.242.68.4: icmp_seq=1 ttl=45 time=15.458 ms
64 bytes from 110.242.68.4: icmp_seq=2 ttl=45 time=21.048 ms
64 bytes from 110.242.68.4: icmp_seq=3 ttl=45 time=18.368 ms
64 bytes from 110.242.68.4: icmp_seq=4 ttl=45 time=19.718 ms

--- www.baidu.com ping statistics ---
5 packets transmitted, 5 packets received, 0.00% packet loss
round-trip min/avg/max/stddev = 12.721/17.463/21.048/3.009 ms
```

# Notice
If you are **time sensitive**, please do not use `asynchronous ping program`, because if there are a large number of asynchronous events waiting to wake up, it will cause inaccurate calculation time. You can directly use the `ping command` of the operating system.


# License
This project is licensed under the [MIT license].

[MIT license]: https://github.com/kolapapa/surge-ping/blob/main/LICENSE
