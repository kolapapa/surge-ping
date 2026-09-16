//! Regression tests for the P0 concurrency defects:
//!
//! 1. dropping one `Client` clone destroyed every other clone;
//! 2. a duplicate request destroyed the request already in flight;
//! 3. cleanup was keyed by sequence alone, so a finished request could cancel a
//!    different in-flight one, and a cancelled `ping` leaked its registration.
//!
//! The socket-free invariants are covered by the unit tests in `src/client.rs`;
//! these exercise the same guarantees through the public API.

use std::time::{Duration, Instant};

use surge_ping::{Client, Config, PingIdentifier, PingSequence, SurgeError};

/// TEST-NET-1 (RFC 5737): never answers, so a ping to it stays parked on its
/// waiter until it times out.
const BLACKHOLE: &str = "192.0.2.1";
const LOCALHOST: &str = "127.0.0.1";

/// Some sandboxes cannot emit packets towards TEST-NET-1 at all. Those
/// environments cannot host the "request parked in flight" tests, so skip them
/// rather than report a failure that says nothing about this crate.
async fn blackhole_parks(client: &Client) -> bool {
    let mut pinger = client
        .pinger(BLACKHOLE.parse().unwrap(), PingIdentifier(0xfff0))
        .await;
    pinger.timeout(Duration::from_millis(200));
    match pinger.ping(PingSequence(0xffff), &[0; 8]).await {
        Err(SurgeError::Timeout { .. }) => true,
        other => {
            eprintln!("skipping: {} is not usable here ({:?})", BLACKHOLE, other);
            false
        }
    }
}

/// Regression 1: `Client::drop` used to mark the shared reply map destroyed
/// unconditionally, so releasing any clone broke every remaining handle.
#[tokio::test]
async fn dropping_one_client_clone_does_not_destroy_the_others() {
    let client = Client::new(&Config::default()).unwrap();

    let clone = client.clone();
    drop(clone);

    let mut pinger = client
        .pinger(LOCALHOST.parse().unwrap(), PingIdentifier(0xa001))
        .await;
    pinger.timeout(Duration::from_millis(500));

    let result = pinger.ping(PingSequence(0), &[0; 8]).await;
    assert!(
        !matches!(result, Err(SurgeError::ClientDestroyed)),
        "dropping a clone must not destroy the surviving client (got {:?})",
        result
    );
}

/// The same, one level deeper: a clone handed to a task and dropped there must
/// not disturb the parent, which is the pattern `examples/multi_ping.rs` uses.
#[tokio::test]
async fn a_clone_dropped_in_a_task_does_not_destroy_the_parent() {
    let client = Client::new(&Config::default()).unwrap();

    let moved = client.clone();
    tokio::spawn(async move {
        let _ = moved
            .pinger(LOCALHOST.parse().unwrap(), PingIdentifier(0xa002))
            .await;
        // `moved` is dropped here.
    })
    .await
    .unwrap();

    let mut pinger = client
        .pinger(LOCALHOST.parse().unwrap(), PingIdentifier(0xa003))
        .await;
    pinger.timeout(Duration::from_millis(500));

    let result = pinger.ping(PingSequence(0), &[0; 8]).await;
    assert!(
        !matches!(result, Err(SurgeError::ClientDestroyed)),
        "a clone dropped inside a task must not destroy the parent (got {:?})",
        result
    );
}

/// Dropping the *last* handle must still shut the client down — the behaviour
/// the unconditional `mark_destroyed` was there to provide.
#[tokio::test]
async fn dropping_every_client_clone_still_destroys_the_client() {
    let client = Client::new(&Config::default()).unwrap();
    let clone = client.clone();

    let mut pinger = clone
        .pinger(LOCALHOST.parse().unwrap(), PingIdentifier(0xa004))
        .await;

    drop(client);
    drop(clone);

    assert!(
        matches!(
            pinger.ping(PingSequence(0), &[0; 8]).await,
            Err(SurgeError::ClientDestroyed)
        ),
        "dropping the last handle must destroy the client"
    );
}

/// Regression 2: the duplicate request used to replace the original sender, so
/// the in-flight request failed with `NetworkError` instead of timing out.
#[tokio::test]
async fn a_duplicate_request_does_not_kill_the_one_in_flight() {
    let client = Client::new(&Config::default()).unwrap();
    if !blackhole_parks(&client).await {
        return;
    }
    let host = BLACKHOLE.parse().unwrap();
    let ident = PingIdentifier(0xb001);

    let mut first = client.pinger(host, ident).await;
    first.timeout(Duration::from_secs(3));
    let in_flight = tokio::spawn(async move { first.ping(PingSequence(0), &[0; 8]).await });

    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut second = client.pinger(host, ident).await;
    second.timeout(Duration::from_secs(3));
    let duplicate = second.ping(PingSequence(0), &[0; 8]).await;
    assert!(
        matches!(duplicate, Err(SurgeError::IdenticalRequests { .. })),
        "the duplicate must be rejected (got {:?})",
        duplicate
    );

    let original = in_flight.await.unwrap();
    assert!(
        matches!(original, Err(SurgeError::Timeout { .. })),
        "the original request must be left alone and time out (got {:?})",
        original
    );
}

/// Regression 3a: `Pinger::drop` removed `(host, ident, last_sequence)` without
/// checking ownership, cancelling whichever request held that token at the time.
#[tokio::test]
async fn dropping_a_finished_pinger_does_not_cancel_another() {
    let client = Client::new(&Config::default()).unwrap();
    if !blackhole_parks(&client).await {
        return;
    }
    let host = BLACKHOLE.parse().unwrap();
    let ident = PingIdentifier(0xb002);

    // First pinger completes (times out) and is kept alive for now.
    let mut finished = client.pinger(host, ident).await;
    finished.timeout(Duration::from_millis(200));
    assert!(matches!(
        finished.ping(PingSequence(0), &[0; 8]).await,
        Err(SurgeError::Timeout { .. })
    ));

    // A second pinger claims the same token and parks on it.
    let mut waiting = client.pinger(host, ident).await;
    waiting.timeout(Duration::from_secs(2));
    let in_flight = tokio::spawn(async move { waiting.ping(PingSequence(0), &[0; 8]).await });
    tokio::time::sleep(Duration::from_millis(300)).await;

    drop(finished);

    let result = in_flight.await.unwrap();
    assert!(
        matches!(result, Err(SurgeError::Timeout { .. })),
        "dropping an unrelated finished pinger must not cancel this request (got {:?})",
        result
    );
}

/// Regression 4: shutting the client down only flipped a flag, so a request
/// already waiting stayed parked until its own timeout expired — and then
/// reported `NetworkError` rather than saying the client had gone away.
#[tokio::test]
async fn dropping_the_last_client_releases_requests_in_flight() {
    let client = Client::new(&Config::default()).unwrap();
    if !blackhole_parks(&client).await {
        return;
    }

    let mut pinger = client
        .pinger(BLACKHOLE.parse().unwrap(), PingIdentifier(0xb004))
        .await;
    // Long enough that a timeout could not plausibly be what ends this request.
    pinger.timeout(Duration::from_secs(30));
    let in_flight = tokio::spawn(async move { pinger.ping(PingSequence(0), &[0; 8]).await });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let shutdown = Instant::now();
    drop(client);

    let result = tokio::time::timeout(Duration::from_secs(5), in_flight)
        .await
        .expect("the request must not outlive the client it was issued from")
        .unwrap();

    assert!(
        matches!(result, Err(SurgeError::ClientDestroyed)),
        "a request released by shutdown must say so (got {:?})",
        result
    );
    assert!(
        shutdown.elapsed() < Duration::from_secs(1),
        "the request must be released promptly, took {:?}",
        shutdown.elapsed()
    );
}

/// Regression 3b: a cancelled `ping` future used to leave its token registered,
/// so retrying the same sequence failed with `IdenticalRequests` forever.
#[tokio::test]
async fn a_cancelled_ping_releases_its_sequence() {
    let client = Client::new(&Config::default()).unwrap();
    if !blackhole_parks(&client).await {
        return;
    }
    let host = BLACKHOLE.parse().unwrap();
    let ident = PingIdentifier(0xb003);

    let mut pinger = client.pinger(host, ident).await;
    pinger.timeout(Duration::from_secs(30));

    // Cancel the ping while it is waiting for a reply.
    tokio::select! {
        _ = pinger.ping(PingSequence(0), &[0; 8]) => panic!("the blackhole must not reply"),
        _ = tokio::time::sleep(Duration::from_millis(300)) => {}
    }

    // The sequence must be usable again.
    pinger.timeout(Duration::from_millis(200));
    let retry = pinger.ping(PingSequence(0), &[0; 8]).await;
    assert!(
        matches!(retry, Err(SurgeError::Timeout { .. })),
        "the cancelled request must have released its sequence (got {:?})",
        retry
    );
}
