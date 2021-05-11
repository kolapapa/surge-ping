# surge-ping
[![Crates.io](https://img.shields.io/crates/v/surge-ping.svg)](https://crates.io/crates/surge-ping)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kolapapa/surge-ping/blob/main/LICENSE)
[![API docs](https://docs.rs/surge-ping/badge.svg)](http://docs.rs/surge-ping)

rust ping libray based on `tokio` + `socket2` + `pnet_packet`.

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

You can send ICMP packets with custom interface
```rust
pinger.bind_device(Some("eth0".as_bytes()))?;
```


### Ping(ICMP)
There are two example programs that you can run on your own.
```shell
$ git clone https://github.com/kolapapa/surge-ping.git
$ cd surge-ping


$ cargo build --example simple
sudo RUST_LOG=info ./target/debug/examples/simple -h www.baidu.com -s 56
INFO  simple > Ok((EchoReply { ttl: Some(48), source: 220.181.38.148, sequence: 0, identifier: 111, size: 64 }, 7.4106ms))

$ cargo build --example cmd
sudo ./target/debug/examples/cmd -h www.baidu.com -c 5
PING www.baidu.com (220.181.38.149): 56 data bytes
64 bytes from 220.181.38.149: icmp_seq=0 ttl=45 time=8.987 ms
64 bytes from 220.181.38.149: icmp_seq=1 ttl=45 time=15.662 ms
64 bytes from 220.181.38.149: icmp_seq=2 ttl=45 time=14.924 ms
64 bytes from 220.181.38.149: icmp_seq=3 ttl=45 time=8.902 ms
64 bytes from 220.181.38.149: icmp_seq=4 ttl=45 time=11.281 ms

--- www.baidu.com ping statistics ---
5 packets transmitted, 5 packets received, 0.00% packet loss
round-trip min/avg/max/stddev = 8.902/11.951/15.662/2.868 ms
```

### Traceroute(ICMP)
At present, a sample version of `Traceroute` is implemented(only IPv4 is supported), which can be viewed through the branch of [traceroute](https://github.com/kolapapa/surge-ping/tree/traceroute)

# Notice
If you are **time sensitive**, please do not use `asynchronous ping program`, because if there are a large number of asynchronous events waiting to wake up, it will cause inaccurate calculation time. You can directly use the `ping command` of the operating system.


# License
This project is licensed under the [MIT license].

[MIT license]: https://github.com/kolapapa/surge-ping/blob/main/LICENSE
