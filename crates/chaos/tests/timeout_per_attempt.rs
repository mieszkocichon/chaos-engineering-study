//! Regression: like JS `_fetch`, the request timeout is re-read on every retry attempt, so a
//! parameter change (`/control` for central, the 2 s decision tick for MAS) applies to retries
//! already in flight.

use std::time::{Duration, Instant};

use chaos::strategy::central::CentralStrategy;
use chaos::strategy::ResilienceStrategy;
use chaos::topology::Downstream;
use serde_json::json;
use tokio::io::AsyncReadExt;

/// A downstream that accepts connections and never answers.
async fn black_hole() -> String {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = l.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while s.read(&mut buf).await.unwrap_or(0) > 0 {}
            });
        }
    });
    format!("http://{addr}/data")
}

#[tokio::test(flavor = "multi_thread")]
async fn central_retry_uses_timeout_updated_during_first_attempt() {
    let s = std::sync::Arc::new(CentralStrategy::new());
    // retryCount 1 (2 attempts), backoff base 200 ms, first attempt times out after 3000 ms
    s.apply_update(&json!({ "retryCount": 1, "timeoutMs": 3000 }));
    let ds = vec![Downstream { id: "slow".into(), url: black_hole().await }];

    let s2 = s.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        s2.apply_update(&json!({ "timeoutMs": 1000 })); // lands during attempt 1
    });

    let client = reqwest::Client::new();
    let t0 = Instant::now();
    let r = s.call_downstreams(&client, &ds).await.unwrap();
    let took = t0.elapsed();
    assert_eq!(r[0]["degraded"], true);
    // re-read per attempt: 3000 + 200 + 1000 ≈ 4.2 s; snapshotted: 3000 + 200 + 3000 ≈ 6.2 s
    assert!(took < Duration::from_millis(5200), "retry kept the stale timeout: {took:?}");
    assert!(took > Duration::from_millis(4000), "{took:?}");
}
