//! Gossip client – port of `GossipClient.pushTo` (`lib/gossip.js`).

use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::metrics::HealthSummary;
use crate::now_ms;

pub fn summary_map(h: &HealthSummary) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("errorRate".into(), json!(h.error_rate));
    m.insert("avgLatency".into(), json!(h.avg_latency));
    m.insert("load".into(), json!(h.load));
    m
}

/// POST `{fromId, ...payload, timestamp}` with a 1 s timeout; failures are swallowed.
pub async fn push_to(client: &reqwest::Client, url: &str, from_id: &str, payload: &Map<String, Value>) {
    let mut body = Map::new();
    body.insert("fromId".into(), json!(from_id));
    body.extend(payload.clone());
    body.insert("timestamp".into(), json!(now_ms()));
    let _ = client.post(url).json(&Value::Object(body)).timeout(Duration::from_millis(1000)).send().await;
}
