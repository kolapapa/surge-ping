# surge-ping
rust ping libray based on `tokio 1.0` + `socket2` + `packet`

### Example usage
```rust
use std::time::Duration;

use surge_ping::Pinger;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut pinger = Pinger::new("114.114.114.114".parse()?)?;
    pinger.timeout(Duration::from_secs(1));
    for idx in 0..10 {
        let (reply, dur) = pinger.ping(idx).await?;
        println!(
            "{} bytes from {}: icmp_seq={} ttl={} time={:?}",
            reply.size, reply.source, reply.sequence, reply.ttl, dur
        );
    }
    Ok(())
}

```

#### Simple run
There are two example programs that you can run on your own.
```shell
$ git clone https://github.com/kolapapa/surge-ping.git
$ cd surge-ping


$ cargo build --example simple
sudo RUST_LOG=info ./target/debug/examples/simple -h www.baidu.com -s 56
INFO  simple > Ok((EchoReply { ttl: 48, source: 220.181.38.148, sequence: 0, size: 56 }, 7.4106ms))

$ cargo build --example cmd
sudo ./target/debug/examples/cmd -h www.baidu.com -c 3
PING www.baidu.com (220.181.38.149): 56 data bytes
56 bytes from 220.181.38.149: icmp_seq=0 ttl=45 time=8.355 ms
56 bytes from 220.181.38.149: icmp_seq=1 ttl=45 time=12.577 ms
56 bytes from 220.181.38.149: icmp_seq=2 ttl=45 time=110.880 ms

--- www.baidu.com ping statistics ---
3 packets transmitted, 3 packets received, 0.00% packet loss
round-trip min/avg/max/stddev = 8.355/43.937/110.880/47.367 ms
```

# License
This project is licensed under either of

Apache License, Version 2.0, (LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0).
