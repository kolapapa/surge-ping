use std::time::Duration;
use surge_ping::{Client, Config, PingIdentifier, PingSequence, SurgeError};

#[tokio::test]
async fn test_pinger_survives_client_drop() {
    let config = Config::default();
    let client = Client::new(&config).unwrap();

    // Create a pinger while client is alive
    let mut pinger = client
        .pinger("8.8.8.8".parse().unwrap(), PingIdentifier(42))
        .await;

    // Set a short timeout so the test runs quickly
    pinger.timeout(Duration::from_millis(200));

    // Drop the client
    drop(client);

    // Give some time for drop to complete and potential background task cleanup (if bug existed)
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Now try to ping - should NO LONGER get ClientDestroyed error
    // It should work (Ok) or Timeout, depending on environment/permissions.
    let result = pinger.ping(PingSequence(0), &[0; 8]).await;

    match result {
        Err(SurgeError::ClientDestroyed) => {
            panic!("Pinger failed with ClientDestroyed! The background task was killed prematurely.");
        }
        Ok(_) => {
            // Success!
        }
        Err(SurgeError::Timeout { .. }) => {
            // Success! (Timeout means the mechanism worked, just no reply)
        }
        Err(e) => {
            // Other network errors are also "Success" in terms of "Client not destroyed"
            println!("Got other error (acceptable): {:?}", e);
        }
    }
}
