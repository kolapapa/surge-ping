# surge
same as `ping`, based on `tokio` + `socket2` + `packet`

#### Simple run
```shell
$ git clone https://github.com/kolapapa/surge.git
$ cd surge
$ cargo build
sudo ./target/debug/surge -h www.baidu.com -s 56
56 bytes from 110.242.68.3: icmp_seq=0 ttl=44 time=13.434519ms
56 bytes from 110.242.68.3: icmp_seq=1 ttl=44 time=82.91822ms
56 bytes from 110.242.68.3: icmp_seq=2 ttl=44 time=17.331204ms
56 bytes from 110.242.68.3: icmp_seq=3 ttl=44 time=15.219842ms
56 bytes from 110.242.68.3: icmp_seq=4 ttl=44 time=14.833708ms
56 bytes from 110.242.68.3: icmp_seq=5 ttl=44 time=17.569047ms
56 bytes from 110.242.68.3: icmp_seq=6 ttl=44 time=13.891027ms
...

```