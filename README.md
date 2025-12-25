# surge-ping

A Ping (ICMP) detection tool, you can personalize the Ping parameters. Since version `0.4.0`, a new `Client` data structure
has been added. This structure wraps the `socket` implementation and can be passed between any task cheaply. If you have multiple
addresses to detect, you can easily complete it by creating only one system socket(Thanks @wladwm).

[![Crates.io](https://img.shields.io/crates/v/surge-ping.svg)](https://crates.io/crates/surge-ping)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kolapapa/surge-ping/blob/main/LICENSE)
[![API docs](https://docs.rs/surge-ping/badge.svg)](http://docs.rs/surge-ping)

rust ping libray based on `tokio` + `socket2` + `pnet_packet`.

## Example

simple usage:

```rust
/*
Cargo.toml

[dependencies]
surge-ping = "last version"
tokio = { version = "1.21.2", features = ["full"] }
*/

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let payload = [0; 8];

    let (_packet, duration) = surge_ping::ping("127.0.0.1".parse()?, &payload).await?;

    println!("Ping took {:.3?}", duration);

    Ok(())
}

```

multi address usage: [multi_ping.rs](https://github.com/kolapapa/surge-ping/blob/main/examples/multi_ping.rs)

### Ping(ICMP)
There are three example programs that you can run on your own.

```shell
$ git clone https://github.com/kolapapa/surge-ping.git
$ cd surge-ping


$ cargo run --example simple -- -h 8.8.8.8 -s 56
V4(Icmpv4Packet { source: 8.8.8.8, destination: 10.1.40.79, ttl: 53, icmp_type: IcmpType(0), icmp_code: IcmpCode(0), size: 64, real_dest: 8.8.8.8, identifier: 111, sequence: 0 }) 112.36ms


$ cargo run --example cmd -- -h google.com -c 5
PING google.com (172.217.24.238): 56 data bytes
64 bytes from 172.217.24.238: icmp_seq=0 ttl=115 time=109.902 ms
64 bytes from 172.217.24.238: icmp_seq=1 ttl=115 time=73.684 ms
64 bytes from 172.217.24.238: icmp_seq=2 ttl=115 time=65.865 ms
64 bytes from 172.217.24.238: icmp_seq=3 ttl=115 time=66.328 ms
64 bytes from 172.217.24.238: icmp_seq=4 ttl=115 time=68.707 ms

--- google.com ping statistics ---
5 packets transmitted, 5 packets received, 0.00% packet loss
round-trip min/avg/max/stddev = 65.865/76.897/109.902/16.734 ms
```

## Notice

If you are **time sensitive**, please do not use `asynchronous ping program`, because if there are a large number of asynchronous events waiting to wake up, it will cause inaccurate calculation time. You can directly use the `ping command` of the operating system.

## Non-privileged Ping (Linux)

On Linux systems (kernel 2.6.30+), `surge-ping` supports **non-privileged ICMP** datagram sockets, allowing ping operations without root privileges or `CAP_NET_RAW` capability.

### How it works

The library automatically tries socket types in this order:
1. **DGRAM socket** (non-privileged, works on Linux with ICMP ECHO restriction)
2. **RAW socket** (requires root/CAP_NET_RAW, fallback on other systems)

This means it works out of the box for most Linux users without special permissions.

### System Configuration

If non-privileged ping is not working, check your system configuration:

```bash
# Check if non-privileged ICMP is enabled
sysctl net.ipv4.ping_group_range

# Typical output: "0   2147483647" (enabled for all groups)
# If output is "1   0", it's disabled
```

To enable non-privileged ping temporarily:
```bash
sudo sysctl -w net.ipv4.ping_group_range="0 2147483647"
```

To make the change persistent:
```bash
echo "net.ipv4.ping_group_range=0 2147483647" | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

### Troubleshooting

If you encounter "Permission denied" errors:
1. Check `net.ipv4.ping_group_range` as shown above
2. Some container environments may have additional restrictions
3. As a fallback, run with `sudo` or add `CAP_NET_RAW` capability:
   ```bash
   sudo setcap cap_net_raw+ep your-binary
   ```

## License

This project is licensed under the [MIT license].

[MIT license]: https://github.com/kolapapa/surge-ping/blob/main/LICENSE
