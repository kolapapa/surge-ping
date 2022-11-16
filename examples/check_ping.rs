
use cfg_if::cfg_if;


fn main() {

    cfg_if! {
        if #[cfg(any(target_os = "linux"))] {
            use surge_ping::{ALLOW_IPV4_RAW_SOCKET, ALLOW_IPV4_UNPRIVILEGED_ICMP};
            use surge_ping::{ALLOW_IPV6_RAW_SOCKET, ALLOW_IPV6_UNPRIVILEGED_ICMP};
        
            println!(
                "ALLOW_IPV4_RAW_SOCKET:          {:?}",
                *ALLOW_IPV4_RAW_SOCKET
            );
            println!(
                "ALLOW_IPV4_UNPRIVILEGED_ICMP:   {:?}",
                *ALLOW_IPV4_UNPRIVILEGED_ICMP
            );
        
            println!(
                "ALLOW_IPV6_RAW_SOCKET:          {:?}",
                *ALLOW_IPV6_RAW_SOCKET
            );
            println!(
                "ALLOW_IPV6_UNPRIVILEGED_ICMP:   {:?}",
                *ALLOW_IPV6_UNPRIVILEGED_ICMP
            );
        } else {
            println!("unnecessary");
        }
    }
}
