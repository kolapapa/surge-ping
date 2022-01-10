use std::net::SocketAddr;

use socket2::SockAddr;

use crate::ICMP;

#[derive(Debug, Default)]
pub struct Config {
    pub kind: ICMP,
    pub bind: Option<SockAddr>,
    pub interface: Option<String>,
    pub ttl: Option<u32>,
    pub fib: Option<u32>,
}

impl Config {
    /// A structure that can be specially configured for socket.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }
}

#[derive(Debug, Default)]
pub struct ConfigBuilder {
    kind: ICMP,
    bind: Option<SockAddr>,
    interface: Option<String>,
    ttl: Option<u32>,
    fib: Option<u32>,
}

impl ConfigBuilder {
    pub fn bind(mut self, bind: SocketAddr) -> Self {
        self.bind = Some(SockAddr::from(bind));
        self
    }

    pub fn interface(mut self, interface: &str) -> Self {
        self.interface = Some(interface.to_string());
        self
    }

    pub fn ttl(mut self, ttl: u32) -> Self {
        self.ttl = Some(ttl);
        self
    }

    pub fn fib(mut self, fib: u32) -> Self {
        self.fib = Some(fib);
        self
    }

    pub fn kind(mut self, kind: ICMP) -> Self {
        self.kind = kind;
        self
    }

    pub fn build(self) -> Config {
        Config {
            kind: self.kind,
            bind: self.bind,
            interface: self.interface,
            ttl: self.ttl,
            fib: self.fib,
        }
    }
}
