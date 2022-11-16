use cfg_if::cfg_if;


cfg_if! {
    if #[cfg(any(target_os = "linux"))] {

        use once_cell::sync::Lazy;
        use socket2::{Domain, Protocol, Socket, Type};
        use crate::ICMP;
        use std::{io, net::IpAddr};
        
        pub trait CheckAllowUnprivilegedIcmp {
            fn allow_unprivileged_icmp(&self) -> bool;
        }
        
        
        pub trait CheckAllowRawSocket {
            fn allow_raw_socket(&self) -> bool;
        }
        
        impl CheckAllowUnprivilegedIcmp for ICMP {
            fn allow_unprivileged_icmp(&self) -> bool {
                match self {
                    ICMP::V4 => *ALLOW_IPV4_UNPRIVILEGED_ICMP,
                    ICMP::V6 => *ALLOW_IPV6_UNPRIVILEGED_ICMP
                }
            }
        }
        
        impl CheckAllowRawSocket for ICMP {
            #[inline]
            fn allow_raw_socket(&self) -> bool {
                match self {
                    ICMP::V4 => *ALLOW_IPV4_RAW_SOCKET,
                    ICMP::V6 => *ALLOW_IPV6_RAW_SOCKET
                }
            }
        }
        
        impl CheckAllowUnprivilegedIcmp for IpAddr {
            #[inline]
            fn allow_unprivileged_icmp(&self) -> bool {
                match self {
                    IpAddr::V4(_) => *ALLOW_IPV4_UNPRIVILEGED_ICMP,
                    IpAddr::V6(_) => *ALLOW_IPV6_UNPRIVILEGED_ICMP,
                }
            }
        }
        
        impl CheckAllowRawSocket for IpAddr {
            #[inline]
            fn allow_raw_socket(&self) -> bool {
                match self {
                    IpAddr::V4(_) => *ALLOW_IPV4_RAW_SOCKET,
                    IpAddr::V6(_) => *ALLOW_IPV6_RAW_SOCKET,
                }
            }
        }
        
        
        
        
        pub static ALLOW_IPV4_UNPRIVILEGED_ICMP: Lazy<bool> = Lazy::new(|| {
            allow_unprivileged_icmp(Domain::IPV4, Protocol::ICMPV4)
        });
        
        pub static ALLOW_IPV4_RAW_SOCKET: Lazy<bool> =
            Lazy::new(|| allow_raw_socket(Domain::IPV4, Protocol::ICMPV4));
        
        
        pub static ALLOW_IPV6_UNPRIVILEGED_ICMP: Lazy<bool> = Lazy::new(|| {
            allow_unprivileged_icmp(Domain::IPV6, Protocol::ICMPV6)
        });
        
        pub static ALLOW_IPV6_RAW_SOCKET: Lazy<bool> =
            Lazy::new(|| allow_raw_socket(Domain::IPV6, Protocol::ICMPV6));
        
        
        fn allow_unprivileged_icmp(domain: Domain, proto: Protocol) -> bool {
            !is_permission_denied(Socket::new(domain, Type::DGRAM, Some(proto)))
        }
        
        fn allow_raw_socket(domain: Domain, proto: Protocol) -> bool {
            !is_permission_denied(Socket::new(domain, Type::RAW, Some(proto)))
        }
        
        #[inline]
        fn is_permission_denied(res: io::Result<Socket>) -> bool {
            matches!(res, Err(err) if matches!(err.kind(), std::io::ErrorKind::PermissionDenied))
        }

    }


}
