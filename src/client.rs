#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, FromRawSocket, IntoRawSocket, RawSocket};

use std::{
    collections::hash_map::{Entry, HashMap},
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use parking_lot::Mutex;
use socket2::{Domain, Protocol, Socket, Type as SockType};
use tokio::{
    net::UdpSocket,
    sync::oneshot,
    task::{self, JoinHandle},
};
use tracing::debug;

use crate::{
    ICMP, IcmpPacket, PingIdentifier, PingSequence, Pinger, SurgeError,
    config::Config,
    icmp::{icmpv4::Icmpv4Packet, icmpv6::Icmpv6Packet},
};

// Check, if the platform's socket operates with ICMP packets in a casual way
#[macro_export]
macro_rules! is_linux_icmp_socket {
    ($sock_type:expr) => {
        if ($sock_type == socket2::Type::DGRAM
            && cfg!(not(any(target_os = "linux", target_os = "android"))))
            || $sock_type == socket2::Type::RAW
        {
            false
        } else {
            true
        }
    };
}

#[derive(Clone)]
pub struct AsyncSocket {
    inner: Arc<UdpSocket>,
    sock_type: SockType,
}

impl AsyncSocket {
    pub fn new(config: &Config) -> io::Result<Self> {
        let (sock_type, socket) = Self::create_socket(config)?;

        socket.set_nonblocking(true)?;
        if let Some(sock_addr) = &config.bind {
            socket.bind(sock_addr)?;
        }
        #[cfg(any(target_os = "android", target_os = "fuchsia", target_os = "linux"))]
        if let Some(interface) = &config.interface {
            socket.bind_device(Some(interface.as_bytes()))?;
        }
        #[cfg(any(
            target_os = "ios",
            target_os = "visionos",
            target_os = "macos",
            target_os = "tvos",
            target_os = "watchos",
            target_os = "illumos",
            target_os = "solaris",
            target_os = "linux",
            target_os = "android",
        ))]
        {
            if config.interface_index.is_some() {
                match config.kind {
                    ICMP::V4 => socket.bind_device_by_index_v4(config.interface_index)?,
                    ICMP::V6 => socket.bind_device_by_index_v6(config.interface_index)?,
                }
            }
        }
        if let Some(ttl) = config.ttl {
            match config.kind {
                ICMP::V4 => socket.set_ttl_v4(ttl)?,
                ICMP::V6 => socket.set_unicast_hops_v6(ttl)?,
            }
        }
        #[cfg(target_os = "freebsd")]
        if let Some(fib) = config.fib {
            socket.set_fib(fib)?;
        }
        #[cfg(windows)]
        let socket = UdpSocket::from_std(unsafe {
            std::net::UdpSocket::from_raw_socket(socket.into_raw_socket())
        })?;
        #[cfg(unix)]
        let socket =
            UdpSocket::from_std(unsafe { std::net::UdpSocket::from_raw_fd(socket.into_raw_fd()) })?;
        Ok(Self {
            inner: Arc::new(socket),
            sock_type,
        })
    }

    fn create_socket(config: &Config) -> io::Result<(SockType, Socket)> {
        let (domain, proto) = match config.kind {
            ICMP::V4 => (Domain::IPV4, Some(Protocol::ICMPV4)),
            ICMP::V6 => (Domain::IPV6, Some(Protocol::ICMPV6)),
        };

        let first_err = match Socket::new(domain, config.sock_type_hint, proto) {
            Ok(sock) => return Ok((config.sock_type_hint, sock)),
            Err(err) => err,
        };

        let fallback_type = if config.sock_type_hint == SockType::DGRAM {
            SockType::RAW
        } else {
            SockType::DGRAM
        };

        debug!(
            "error opening {:?} type socket, trying {:?}: {:?}",
            config.sock_type_hint, fallback_type, first_err
        );

        match Socket::new(domain, fallback_type, proto) {
            Ok(sock) => Ok((fallback_type, sock)),
            Err(_second_err) => {
                #[cfg(all(
                    target_os = "linux",
                    any(
                        target_arch = "x86",
                        target_arch = "x86_64",
                        target_arch = "arm",
                        target_arch = "aarch64"
                    )
                ))]
                {
                    if config.sock_type_hint == SockType::DGRAM
                        && first_err.kind() == io::ErrorKind::PermissionDenied
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            format!(
                                "Permission denied creating ICMP socket. On Linux, you may need to:\n\
                                1. Enable non-privileged ICMP: `sudo sysctl -w net.ipv{}.ping_group_range=\"0 2147483647\"`\n\
                                2. Run with sudo or set CAP_NET_RAW: `sudo setcap cap_net_raw+ep <binary>`\n\
                                Original error: {}",
                                if config.kind == ICMP::V4 { "4" } else { "6" },
                                first_err
                            ),
                        ));
                    }
                }

                Err(first_err)
            }
        }
    }

    pub async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.inner.recv_from(buf).await
    }

    pub async fn send_to(&self, buf: &mut [u8], target: &SocketAddr) -> io::Result<usize> {
        self.inner.send_to(buf, target).await
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    pub fn get_type(&self) -> SockType {
        self.sock_type
    }

    #[cfg(unix)]
    pub fn get_native_sock(&self) -> RawFd {
        self.inner.as_raw_fd()
    }

    #[cfg(windows)]
    pub fn get_native_sock(&self) -> RawSocket {
        self.inner.as_raw_socket()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct ReplyToken(IpAddr, Option<PingIdentifier>, PingSequence);

pub(crate) struct Reply {
    pub timestamp: Instant,
    pub packet: IcmpPacket,
}

/// A registered waiter. The `id` makes every registration unique, so a
/// [`ReplyWaiter`] can only ever unregister the exact entry it created and
/// never one a later request installed under the same token.
struct Registration {
    id: u64,
    tx: oneshot::Sender<Reply>,
}

/// Liveness and the waiter table live behind the same lock, so a client cannot
/// be shut down in the window between "is it still alive?" and "register me".
struct ReplyMapState {
    alive: bool,
    waiters: HashMap<ReplyToken, Registration>,
}

#[derive(Clone)]
pub(crate) struct ReplyMap {
    state: Arc<Mutex<ReplyMapState>>,
    next_id: Arc<AtomicU64>,
}

impl Default for ReplyMap {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(ReplyMapState {
                alive: true,
                waiters: HashMap::new(),
            })),
            next_id: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl ReplyMap {
    /// Register to wait for a reply from host with ident and sequence number.
    /// If there is already someone waiting for this specific reply then an
    /// error is returned and the existing waiter is left untouched.
    pub fn new_waiter(
        &self,
        host: IpAddr,
        ident: Option<PingIdentifier>,
        seq: PingSequence,
    ) -> Result<ReplyWaiter, SurgeError> {
        let token = ReplyToken(host, ident, seq);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();

        {
            let mut state = self.state.lock();
            // Checked under the same lock that shutdown takes, so a client that
            // goes away concurrently can never leave this registration stranded.
            if !state.alive {
                return Err(SurgeError::ClientDestroyed);
            }
            match state.waiters.entry(token) {
                // Somebody is already waiting for this exact reply. Do not
                // disturb them: leave their sender in place, reject the newcomer.
                Entry::Occupied(_) => {
                    return Err(SurgeError::IdenticalRequests { host, ident, seq });
                }
                Entry::Vacant(vacant) => vacant.insert(Registration { id, tx }),
            };
        }

        Ok(ReplyWaiter {
            map: self.clone(),
            token,
            id: Some(id),
            rx,
        })
    }

    /// Take the waiter for an incoming reply, if anyone is waiting for it.
    pub(crate) fn remove(
        &self,
        host: IpAddr,
        ident: Option<PingIdentifier>,
        seq: PingSequence,
    ) -> Option<oneshot::Sender<Reply>> {
        self.state
            .lock()
            .waiters
            .remove(&ReplyToken(host, ident, seq))
            .map(|registration| registration.tx)
    }

    /// Remove a registration only if it is still the one identified by `id`.
    fn remove_registration(&self, token: &ReplyToken, id: u64) {
        let mut state = self.state.lock();
        if let Entry::Occupied(occupied) = state.waiters.entry(*token) {
            if occupied.get().id == id {
                occupied.remove();
            }
        }
    }

    /// Shut the map down: mark it destroyed and release every pending waiter,
    /// so requests in flight fail immediately instead of running to timeout.
    /// Called once the last `Client` handle goes away.
    pub(crate) fn shutdown(&self) {
        // Marking and draining happen under one lock, so no request can slip in
        // afterwards and wait for a reply that will never be dispatched.
        let waiters = {
            let mut state = self.state.lock();
            state.alive = false;
            std::mem::take(&mut state.waiters)
        };
        // Dropped outside the lock: closing each channel wakes its waiter.
        drop(waiters);
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.state.lock().alive
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.state.lock().waiters.len()
    }
}

/// RAII registration guard for a pending reply.
///
/// The token stays registered for exactly as long as this guard lives, so a
/// `ping` future that is cancelled at *any* await point still unregisters
/// itself. [`ReplyWaiter::disarm`] is used once a reply has been delivered, at
/// which point the receiving task has already removed the entry.
pub(crate) struct ReplyWaiter {
    map: ReplyMap,
    token: ReplyToken,
    /// `None` once the guard has been disarmed.
    id: Option<u64>,
    rx: oneshot::Receiver<Reply>,
}

impl ReplyWaiter {
    /// Wait for the reply. Takes `&mut self` so the guard outlives the await
    /// and still cleans up if the surrounding future is dropped.
    pub(crate) async fn wait(&mut self) -> Result<Reply, SurgeError> {
        match (&mut self.rx).await {
            Ok(reply) => Ok(reply),
            // Our sender was dropped without delivering anything. Nothing in
            // the dispatch path drops a sender without sending, so this means
            // the client shut down and drained its waiters.
            Err(_) => {
                if self.map.is_alive() {
                    Err(SurgeError::NetworkError)
                } else {
                    Err(SurgeError::ClientDestroyed)
                }
            }
        }
    }

    /// Give up ownership of the registration without removing it.
    pub(crate) fn disarm(&mut self) {
        self.id = None;
    }
}

impl Drop for ReplyWaiter {
    fn drop(&mut self) {
        if let Some(id) = self.id.take() {
            self.map.remove_registration(&self.token, id);
        }
    }
}

/// The shared state behind a `Client`. Owning it in a single `Arc` means the
/// socket, the reply map and the receiving task all live exactly as long as the
/// last `Client` handle — cloning and dropping individual handles is inert.
struct ClientInner {
    socket: AsyncSocket,
    reply_map: ReplyMap,
    recv: JoinHandle<()>,
}

impl Drop for ClientInner {
    fn drop(&mut self) {
        // The last `Client` handle is gone. Release everyone waiting for a
        // reply that can no longer arrive, and make any further ping fail fast
        // with `ClientDestroyed` rather than run to timeout.
        self.reply_map.shutdown();
        self.recv.abort();
    }
}

///
/// If you want to pass the `Client` in the task, please wrap it with `Arc`: `Arc<Client>`.
/// and can realize the simultaneous ping of multiple addresses when only one `socket` is created.
///
/// `Client` is cheap to clone and every clone is equivalent: dropping one has no
/// effect on the others, and the underlying socket and receiving task are torn
/// down only when the last clone is dropped.
///
#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

impl Client {
    /// A client is generated according to the configuration. In fact, a `AsyncSocket` is wrapped inside,
    /// and you can clone to any `task` at will.
    pub fn new(config: &Config) -> io::Result<Self> {
        let socket = AsyncSocket::new(config)?;
        let reply_map = ReplyMap::default();
        let recv = task::spawn(recv_task(socket.clone(), reply_map.clone()));
        Ok(Self {
            inner: Arc::new(ClientInner {
                socket,
                reply_map,
                recv,
            }),
        })
    }

    /// Create a `Pinger` instance, you can make special configuration for this instance.
    pub async fn pinger(&self, host: IpAddr, ident: PingIdentifier) -> Pinger {
        Pinger::new(
            host,
            ident,
            self.inner.socket.clone(),
            self.inner.reply_map.clone(),
        )
    }

    /// Expose the underlying socket, if user wants to modify any options on it
    pub fn get_socket(&self) -> AsyncSocket {
        self.inner.socket.clone()
    }
}

async fn recv_task(socket: AsyncSocket, reply_map: ReplyMap) {
    let mut buf = [0; 2048];
    loop {
        if let Ok((sz, addr)) = socket.recv_from(&mut buf).await {
            let timestamp = Instant::now();
            let message = &buf[..sz];
            let local_addr = socket.local_addr().unwrap().ip();
            let packet = {
                let result = match addr.ip() {
                    IpAddr::V4(src_addr) => {
                        let local_addr_ip4 = match local_addr {
                            IpAddr::V4(local_addr_ip4) => local_addr_ip4,
                            _ => continue,
                        };

                        Icmpv4Packet::decode(message, socket.sock_type, src_addr, local_addr_ip4)
                            .map(IcmpPacket::V4)
                    }
                    IpAddr::V6(src_addr) => {
                        Icmpv6Packet::decode(message, src_addr).map(IcmpPacket::V6)
                    }
                };
                match result {
                    Ok(packet) => packet,
                    Err(err) => {
                        debug!("error decoding ICMP packet: {:?}", err);
                        continue;
                    }
                }
            };

            let ident = if is_linux_icmp_socket!(socket.get_type()) {
                None
            } else {
                Some(packet.get_identifier())
            };

            // Route by the address the original request went to, not by the
            // sender of this packet: an ICMP error (time exceeded, destination
            // unreachable, ...) is sent by an intermediate router, and the
            // waiter is registered under the target the caller asked for. For
            // echo replies the two are the same address.
            let real_dest = packet.real_destination();
            if let Some(waiter) = reply_map.remove(real_dest, ident, packet.get_sequence()) {
                // If send fails the receiving end has closed. Nothing to do.
                let _ = waiter.send(Reply { timestamp, packet });
            } else {
                debug!(
                    "no one is waiting for ICMP packet from {} for {} ({:?})",
                    addr.ip(),
                    real_dest,
                    packet
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot::error::TryRecvError;

    const HOST: &str = "192.0.2.1";

    fn host() -> IpAddr {
        HOST.parse().unwrap()
    }

    fn ident() -> Option<PingIdentifier> {
        Some(PingIdentifier(1))
    }

    /// Regression: a duplicate registration used to overwrite the sender of the
    /// request already in flight, so the original waiter was dropped and its
    /// `ping` failed with `NetworkError` instead of running to completion.
    #[test]
    fn duplicate_waiter_is_rejected_without_disturbing_the_original() {
        let map = ReplyMap::default();
        let mut first = map.new_waiter(host(), ident(), PingSequence(0)).unwrap();

        let duplicate = map.new_waiter(host(), ident(), PingSequence(0));
        assert!(
            matches!(duplicate, Err(SurgeError::IdenticalRequests { .. })),
            "a second waiter for the same token must be rejected"
        );
        drop(duplicate);

        // The original registration is still the one in the map, and its
        // channel is still open.
        assert_eq!(map.len(), 1);
        assert!(matches!(first.rx.try_recv(), Err(TryRecvError::Empty)));
    }

    /// Regression: cancelling a `ping` future used to leave its token
    /// registered forever, leaking memory and making the sequence unusable.
    #[test]
    fn dropping_the_guard_unregisters_the_token() {
        let map = ReplyMap::default();
        let waiter = map.new_waiter(host(), ident(), PingSequence(0)).unwrap();
        assert_eq!(map.len(), 1);

        drop(waiter);
        assert_eq!(map.len(), 0);

        // And the sequence can be used again.
        map.new_waiter(host(), ident(), PingSequence(0)).unwrap();
    }

    /// Regression: cleanup used to be keyed by token alone, so a finished
    /// request could unregister a *different*, still in-flight request that had
    /// since claimed the same (host, ident, sequence).
    #[test]
    fn a_stale_guard_does_not_unregister_a_later_request() {
        let map = ReplyMap::default();

        // First request is registered, then answered: the receiving task takes
        // its sender out of the map.
        let first = map.new_waiter(host(), ident(), PingSequence(0)).unwrap();
        map.remove(host(), ident(), PingSequence(0)).unwrap();

        // A second request claims the same token and is still waiting.
        let mut second = map.new_waiter(host(), ident(), PingSequence(0)).unwrap();

        // The first guard going away must not touch the second registration.
        drop(first);
        assert_eq!(map.len(), 1);
        assert!(matches!(second.rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn a_disarmed_guard_leaves_the_map_untouched() {
        let map = ReplyMap::default();

        let mut first = map.new_waiter(host(), ident(), PingSequence(0)).unwrap();
        map.remove(host(), ident(), PingSequence(0)).unwrap();
        first.disarm();

        let second = map.new_waiter(host(), ident(), PingSequence(0)).unwrap();
        drop(first);
        assert_eq!(map.len(), 1);
        drop(second);
        assert_eq!(map.len(), 0);
    }

    #[test]
    fn a_destroyed_map_rejects_new_waiters() {
        let map = ReplyMap::default();
        map.shutdown();
        assert!(!map.is_alive());
        assert!(matches!(
            map.new_waiter(host(), ident(), PingSequence(0)),
            Err(SurgeError::ClientDestroyed)
        ));
    }

    /// Regression: shutdown only flipped a flag, so requests already waiting
    /// stayed parked until their own timeout expired.
    #[tokio::test]
    async fn shutdown_releases_every_pending_waiter() {
        let map = ReplyMap::default();
        let mut first = map.new_waiter(host(), ident(), PingSequence(0)).unwrap();
        let mut second = map.new_waiter(host(), ident(), PingSequence(1)).unwrap();
        assert_eq!(map.len(), 2);

        map.shutdown();

        assert_eq!(map.len(), 0, "shutdown must drain the waiter table");
        // Both resolve right away, without anything having to time out.
        assert!(matches!(
            first.wait().await,
            Err(SurgeError::ClientDestroyed)
        ));
        assert!(matches!(
            second.wait().await,
            Err(SurgeError::ClientDestroyed)
        ));
    }

    /// A closed channel on a live client is a genuine network-side failure and
    /// must stay distinguishable from shutdown.
    #[tokio::test]
    async fn a_closed_channel_on_a_live_map_is_a_network_error() {
        let map = ReplyMap::default();
        let mut waiter = map.new_waiter(host(), ident(), PingSequence(0)).unwrap();

        // Take the sender out and drop it, as a failed dispatch would.
        drop(map.remove(host(), ident(), PingSequence(0)).unwrap());

        assert!(map.is_alive());
        assert!(matches!(waiter.wait().await, Err(SurgeError::NetworkError)));
    }

    /// Waiters are keyed per (host, ident, sequence), so unrelated requests
    /// never interfere.
    #[test]
    fn distinct_tokens_are_independent() {
        let map = ReplyMap::default();
        let a = map.new_waiter(host(), ident(), PingSequence(0)).unwrap();
        let _b = map.new_waiter(host(), ident(), PingSequence(1)).unwrap();
        let _c = map
            .new_waiter("192.0.2.2".parse().unwrap(), ident(), PingSequence(0))
            .unwrap();
        assert_eq!(map.len(), 3);

        drop(a);
        assert_eq!(map.len(), 2);
    }
}
