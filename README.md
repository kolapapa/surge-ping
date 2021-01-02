# surge-ping
rust ping libray based on `tokio 1.0` + `socket2` + `packet`

#### Simple run
```shell
$ git clone https://github.com/kolapapa/surge-ping.git
$ cd surge-ping
$ cargo build --example simple
sudo RUST_LOG=info ./target/debug/examples/simple -h www.baidu.com -s 56
INFO  simple > Ok((EchoReply { ttl: 48, source: 220.181.38.148, sequence: 0, size: 56 }, 7.4106ms))

```

# License
This project is licensed under either of

Apache License, Version 2.0, (LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0) at your option.
