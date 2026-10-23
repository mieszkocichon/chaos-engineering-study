//! The "none" strategy – raw HTTP calls with a fixed timeout (3000 ms).
//! Any downstream failure rejects (bubbles up as Err) causing HTTP 502 Bad Gateway.

use futures::future::join_all;
use serde_json::{Value, json};

use super::{BoxFuture, ResilienceStrategy, fetch_json};
use crate::topology::Downstream;

pub struct NoneStrategy {
    pub timeout_ms: f64,
}

impl NoneStrategy {
    pub fn new(timeout_ms: f64) -> Self {
        NoneStrategy { timeout_ms }
    }
}

impl ResilienceStrategy for NoneStrategy {
    fn call_downstreams<'a>(
        &'a self,
        client: &'a reqwest::Client,
        downstreams: &'a [Downstream],
    ) -> BoxFuture<'a, Result<Vec<Value>, String>> {
        Box::pin(async move {
            let calls = downstreams.iter().map(|ds| fetch_json(client, &ds.url, self.timeout_ms));
            join_all(calls).await.into_iter().collect()
        })
    }

    fn stats(&self) -> Value {
        json!({ "strategy": "none", "timeoutMs": self.timeout_ms })
    }
}
