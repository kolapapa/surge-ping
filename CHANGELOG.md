# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This file starts at 0.9.1. For earlier releases see the
[git history](https://github.com/kolapapa/surge-ping/commits/main) and the
[tags](https://github.com/kolapapa/surge-ping/tags).

## [Unreleased]

### Changed

- Echo requests are built in place instead of being copied out of their own
  buffer, halving the allocations per ping.

## [0.9.1] - 2026-09-16

A bug-fix release. No public API was changed or removed.

### Fixed

- **Dropping one `Client` clone no longer breaks the others.** `Client::drop`
  marked the shared reply map destroyed unconditionally, so releasing any clone
  made every surviving clone fail with `ClientDestroyed` — the pattern
  `examples/multi_ping.rs` is built on. The socket, reply map and receiving task
  now live in one `Arc` and are torn down only when the last handle goes. This
  also removes a race where two clones dropped concurrently could leave the
  receiving task running.
- **A duplicate request no longer destroys the request already in flight.**
  Registering a second waiter for the same `(host, identifier, sequence)`
  replaced the first one's sender, so the original `ping` failed with
  `NetworkError` while the duplicate got `IdenticalRequests`. The existing
  waiter is now left untouched.
- **A cancelled `ping` releases its sequence number.** Cancelling the future
  used to leave the registration behind, leaking memory and making that sequence
  unusable until the `Pinger` was dropped.
- **Dropping a `Pinger` no longer cancels an unrelated request.** Cleanup was
  keyed by `(host, identifier, sequence)` alone, so a finished `Pinger` could
  unregister whichever request held that key at the time, failing it with
  `NetworkError`.
- **Requests in flight end as soon as the last `Client` is dropped.** They used
  to stay parked until their own timeout expired and then report `NetworkError`;
  they now return `ClientDestroyed` immediately.
- **ICMP errors reach the request waiting for them.** Time exceeded, destination
  unreachable and similar errors are sent by an intermediate router, but were
  routed on that router's address rather than on the target quoted inside the
  error, so they surfaced as timeouts. This is what a TTL-limited probe depends
  on.
- **The quoted echo header is located correctly** when the original packet
  carried IPv4 header options or IPv6 extension headers. Its offset was assumed
  fixed, so the identifier and sequence were read out of the options or the
  extension headers.
- **The IPv6 identifier and sequence are read from the right offset.** They were
  taken 4 bytes early, off the quoted ICMPv6 type, code and checksum.
- `Icmpv6Packet::get_real_dest` is now set for ICMPv6 error messages; it
  previously kept its `::1` default.

### Added

- `IcmpPacket::real_destination()` — the address the request being answered was
  originally sent to. For an echo reply this is the sender; for an ICMP error it
  is read out of the request quoted inside the error.

### Internal

- Regression tests for each of the fixes above. The reply-map, guard and
  packet-decoding tests need no ICMP socket and are fully deterministic; the
  API-level tests are separate and skip where the environment cannot reach
  TEST-NET-1.
- Tests no longer assume a platform. Two of them asserted that a `Pinger` keeps
  the identifier hint it was given, which does not hold on Linux ICMP sockets
  where the kernel owns the identifier; they now derive the expectation the same
  way the library does.
- A GitHub Actions workflow covering rustfmt, clippy, docs, the declared MSRV of
  1.85.0, `cargo audit`, and a build matrix over Linux, macOS and Windows. Tests
  requiring a real ICMP socket run in their own job.

[Unreleased]: https://github.com/kolapapa/surge-ping/compare/0.9.1...HEAD
[0.9.1]: https://github.com/kolapapa/surge-ping/compare/0.9.0...0.9.1
