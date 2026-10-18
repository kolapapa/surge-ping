//! `Client::new` returns `io::Result<Self>`, which reads as "any failure comes
//! back as `Err`". Outside a Tokio runtime it used to panic instead, because
//! `UdpSocket::from_std` needs the I/O driver. Called from a plain thread with
//! no runtime, it must now return an error rather than panic.

use surge_ping::{Client, Config};

#[test]
fn client_new_without_a_runtime_errors_instead_of_panicking() {
    let result = Client::new(&Config::default());
    assert!(
        result.is_err(),
        "Client::new outside a Tokio runtime must return Err, not construct a client"
    );
}
