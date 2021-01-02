# surge
rust ping libray based on `tokio 1.0` + `socket2` + `packet`

#### Simple run
```shell
$ git clone https://github.com/kolapapa/surge.git
$ cd surge
$ cargo build --example simple
sudo RUST_LOG=info ./target/debug/examples/simple -h www.baidu.com -s 56
INFO  simple > Ok((EchoReply { ttl: 48, source: 220.181.38.148, sequence: 0, size: 56 }, 7.4106ms))

```