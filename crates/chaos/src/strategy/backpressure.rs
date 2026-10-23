//! Pre-emptive backpressure – port of `when-gossip-goes-silent/agent/backpressure-agent.js`
//! (not the stale copy in the repo root).

use std::sync::{Arc, Mutex};

use futures::future::join_all;
use indexmap::IndexMap;
use metastable::js::{js_number, js_to_fixed};
use serde_json::{Map, Value, json};

use super::{BoxFuture, ResilienceStrategy, fetch_json, parse_gossip_in, spawn_every};
use crate::metrics::{HealthSummary, Snapshot};
use crate::topology::{Downstream, Neighbor};
use crate::{gossip, now_ms};

pub const GOSSIP_INTERVAL_MS: u64 = 1000;
pub const REQUEST_TIMEOUT_MS: u64 = 3000;
const THROTTLE_ERROR_RATE: f64 = 0.3;
const THROTTLE_LOAD: f64 = 15.0;
const THROTTLE_LATENCY_MS: f64 = 2000.0;
const LOCAL_FAILURE_THRESHOLD: u64 = 3;
const LOCAL_FAILURE_WINDOW_MS: f64 = 8000.0;
const LOCAL_RECOVERY_PROBE_MS: f64 = 5000.0;
const GOSSIP_STALE_MS: f64 = 5000.0;
const MAX_DECISION_LOG: usize = 500;

#[derive(Clone, Copy, Debug)]
struct NeighborState {
    error_rate: f64,
    avg_latency: f64,
    load: f64,
    timestamp: f64,
}

#[derive(Clone, Copy, Debug)]
struct LocalFailures {
    count: u64,
    window_start_ms: f64,
    last_throttled_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Throttle {
    pub throttle: bool,
    pub source: Option<&'static str>,
    pub reason: Option<String>,
}

impl Throttle {
    fn pass() -> Self {
        Throttle { throttle: false, source: None, reason: None }
    }
}

#[derive(Default)]
pub struct BackpressureAgent {
    pub service_id: String,
    neighbor_state: IndexMap<String, NeighborState>,
    local_failures: IndexMap<String, LocalFailures>,
    throttled_count: u64,
    throttled_by_gossip: u64,
    throttled_by_local_fails: u64,
    passed_count: u64,
    gossip_received: u64,
    throttle_log: Vec<Value>,
}

impl BackpressureAgent {
    pub fn new(service_id: &str) -> Self {
        BackpressureAgent { service_id: service_id.into(), ..Default::default() }
    }

    /// `receiveGossip`: missing fields → 0, missing timestamp → now (`??`, so 0 stays 0).
    pub fn receive_gossip(&mut self, from: &str, error_rate: Option<f64>, avg_latency: Option<f64>, load: Option<f64>, timestamp: Option<f64>, now_ms: u64) {
        self.neighbor_state.insert(
            from.into(),
            NeighborState {
                error_rate: error_rate.unwrap_or(0.0),
                avg_latency: avg_latency.unwrap_or(0.0),
                load: load.unwrap_or(0.0),
                timestamp: timestamp.unwrap_or(now_ms as f64),
            },
        );
        self.gossip_received += 1;
    }

    pub fn should_throttle(&mut self, ds: &str, now_ms: u64) -> Throttle {
        let now = now_ms as f64;
        if let Some(s) = self.neighbor_state.get(ds) {
            if now - s.timestamp <= GOSSIP_STALE_MS {
                let reason = if s.error_rate > THROTTLE_ERROR_RATE {
                    Some(format!(
                        "errorRate={}%>{}%",
                        js_to_fixed(s.error_rate * 100.0, 1),
                        js_number(THROTTLE_ERROR_RATE * 100.0)
                    ))
                } else if s.load > THROTTLE_LOAD {
                    Some(format!("load={}>{}", js_number(s.load), js_number(THROTTLE_LOAD)))
                } else if s.avg_latency > THROTTLE_LATENCY_MS {
                    Some(format!("avgLatency={}ms>{}ms", js_number(s.avg_latency), js_number(THROTTLE_LATENCY_MS)))
                } else {
                    None
                };
                if let Some(r) = reason {
                    return Throttle { throttle: true, source: Some("gossip"), reason: Some(r) };
                }
            }
        }
        if let Some(rec) = self.local_failures.get_mut(ds) {
            if now - rec.window_start_ms > LOCAL_FAILURE_WINDOW_MS {
                rec.count = 0;
                rec.window_start_ms = now;
            }
            if rec.count >= LOCAL_FAILURE_THRESHOLD {
                // let one probe through periodically to detect recovery
                if let Some(last) = rec.last_throttled_ms {
                    if now - last > LOCAL_RECOVERY_PROBE_MS {
                        rec.last_throttled_ms = None;
                        return Throttle::pass();
                    }
                }
                rec.last_throttled_ms = Some(now);
                return Throttle {
                    throttle: true,
                    source: Some("local"),
                    reason: Some(format!("localFailures={}>={LOCAL_FAILURE_THRESHOLD}", rec.count)),
                };
            }
        }
        Throttle::pass()
    }

    pub fn record_local_failure(&mut self, ds: &str, now_ms: u64) {
        let now = now_ms as f64;
        let rec = self.local_failures.entry(ds.into()).or_insert(LocalFailures { count: 0, window_start_ms: now, last_throttled_ms: None });
        if now - rec.window_start_ms > LOCAL_FAILURE_WINDOW_MS {
            rec.count = 0;
            rec.window_start_ms = now;
        }
        rec.count += 1;
    }

    pub fn record_local_success(&mut self, ds: &str, now_ms: u64) {
        if let Some(rec) = self.local_failures.get_mut(ds) {
            rec.count = 0;
            rec.window_start_ms = now_ms as f64;
            rec.last_throttled_ms = None;
        }
    }

    /// Book-keeping for a throttled call; returns the error message.
    pub fn note_throttled(&mut self, ds: &str, t: &Throttle, now_ms: u64) -> String {
        self.throttled_count += 1;
        match t.source {
            Some("gossip") => self.throttled_by_gossip += 1,
            Some("local") => self.throttled_by_local_fails += 1,
            _ => {}
        }
        let reason = t.reason.clone().unwrap_or_default();
        self.throttle_log.push(json!({ "ts": now_ms, "downstreamId": ds, "reason": reason, "source": t.source }));
        if self.throttle_log.len() > MAX_DECISION_LOG {
            self.throttle_log.remove(0);
        }
        format!("BackpressureThrottled: pre-emptive drop for {ds} ({reason})")
    }

    pub fn note_passed(&mut self) {
        self.passed_count += 1;
    }

    pub fn stats(&self, now_ms: u64) -> Value {
        let total = self.throttled_count + self.passed_count;
        let neighbor_states: Map<String, Value> = self
            .neighbor_state
            .iter()
            .map(|(id, s)| {
                (
                    id.clone(),
                    json!({
                        "errorRate": metastable::js::js_fixed_num(s.error_rate, 4),
                        "avgLatency": s.avg_latency,
                        "load": s.load,
                        "ageMs": now_ms as f64 - s.timestamp,
                    }),
                )
            })
            .collect();
        let recent = &self.throttle_log[self.throttle_log.len().saturating_sub(20)..];
        json!({
            "strategy": "backpressure",
            "serviceId": self.service_id,
            "throttledCount": self.throttled_count,
            "throttledByGossip": self.throttled_by_gossip,
            "throttledByLocalFails": self.throttled_by_local_fails,
            "passedCount": self.passed_count,
            "throttleRate": if total > 0 { metastable::js::js_fixed_num(self.throttled_count as f64 / total as f64, 4) } else { 0.0 },
            "gossipReceived": self.gossip_received,
            "neighborStates": neighbor_states,
            "recentThrottles": recent,
        })
    }

    pub fn reset(&mut self) {
        *self = BackpressureAgent::new(&self.service_id);
    }
}

// ---------------------------------------------------------------- strategy runtime

pub struct BackpressureStrategy {
    pub agent: Mutex<BackpressureAgent>,
    pub neighbors: Vec<Neighbor>,
}

impl BackpressureStrategy {
    pub fn new(service_id: &str, neighbors: Vec<Neighbor>) -> Self {
        BackpressureStrategy {
            agent: Mutex::new(BackpressureAgent::new(service_id)),
            neighbors,
        }
    }

    async fn gossip_round(&self, client: &reqwest::Client, base: &HealthSummary) {
        let (from, targets) = {
            let from = self.agent.lock().unwrap().service_id.clone();
            let plain = gossip::summary_map(base);
            let targets: Vec<(Neighbor, Map<String, Value>)> =
                self.neighbors.iter().map(|n| (n.clone(), plain.clone())).collect();
            (from, targets)
        };
        join_all(targets.iter().map(|(n, p)| gossip::push_to(client, &n.gossip_url, &from, p))).await;
    }
}

struct CallErr {
    msg: String,
    throttled: bool,
}

impl From<String> for CallErr {
    fn from(msg: String) -> Self {
        CallErr { msg, throttled: false }
    }
}

impl ResilienceStrategy for BackpressureStrategy {
    fn call_downstreams<'a>(
        &'a self,
        client: &'a reqwest::Client,
        downstreams: &'a [Downstream],
    ) -> BoxFuture<'a, Result<Vec<Value>, String>> {
        Box::pin(async move {
            let calls = downstreams.iter().map(|ds| async move {
                let t = self.agent.lock().unwrap().should_throttle(&ds.id, now_ms());
                if t.throttle {
                    let msg = self.agent.lock().unwrap().note_throttled(&ds.id, &t, now_ms());
                    return Err(CallErr { msg, throttled: true });
                }
                self.agent.lock().unwrap().note_passed();
                let r = fetch_json(client, &ds.url, REQUEST_TIMEOUT_MS as f64).await;
                let mut g = self.agent.lock().unwrap();
                match &r {
                    Ok(_) => g.record_local_success(&ds.id, now_ms()),
                    Err(_) => g.record_local_failure(&ds.id, now_ms()),
                }
                Ok(r?)
            });
            let results = join_all(calls).await;
            Ok(results
                .into_iter()
                .zip(downstreams)
                .map(|(r, ds)| match r {
                    Ok(v) => v,
                    Err(e) => json!({
                        "error": e.msg,
                        "service": ds.id,
                        "throttled": e.throttled,
                        "degraded": !e.throttled,
                    }),
                })
                .collect())
        })
    }

    fn has_gossip_receiver(&self) -> bool {
        true
    }

    fn receive_gossip(&self, body: &Value) {
        let Some((from, g)) = parse_gossip_in(body) else { return };
        self.agent
            .lock()
            .unwrap()
            .receive_gossip(&from, g.error_rate, g.avg_latency, g.load, g.timestamp, now_ms());
    }

    fn stats(&self) -> Value {
        self.agent.lock().unwrap().stats(now_ms())
    }

    fn reset(&self) {
        self.agent.lock().unwrap().reset();
    }

    fn start(
        self: Arc<Self>,
        client: reqwest::Client,
        health: Arc<dyn Fn() -> HealthSummary + Send + Sync>,
        _snapshot: Arc<dyn Fn() -> Snapshot + Send + Sync>,
    ) {
        let me = self.clone();
        spawn_every(GOSSIP_INTERVAL_MS, move || {
            let (me, client, health) = (me.clone(), client.clone(), health.clone());
            async move { me.gossip_round(&client, &health()).await }
        });
    }
}
