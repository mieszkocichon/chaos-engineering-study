//! Centralized controller decision logic – port of `CentralController._decide` in
//! `lib/strategies/central.js`. Snapshots are raw JSON (`snapshot || healthSummary || {}`).

use std::sync::Mutex;

use futures::future::join_all;
use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use super::pipeline::{PipelineConfig, Pipelines};
use super::{BoxFuture, ResilienceStrategy, call_pipelined, format_pipelined_results};
use crate::now_ms;
use crate::resilience::*;
use crate::topology::Downstream;

/// Numeric field with JS `undefined` → NaN semantics (all comparisons false).
fn f(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(f64::NAN)
}

#[derive(Default)]
pub struct CentralDecider {
    per_service: IndexMap<String, Params>,
    pub decision_history: Vec<Value>,
}

impl CentralDecider {
    pub fn decide(&mut self, service_id: &str, all: &IndexMap<String, Value>, now_ms: u64) -> Params {
        let empty = json!({});
        let local = all.get(service_id).unwrap_or(&empty);
        let mut p = *self.per_service.entry(service_id.to_string()).or_default();

        let er = f(local, "errorRate");
        if er > 0.5 {
            p.retry_count = jmax(RETRY.min, p.retry_count - 1.0);
            p.cb_threshold = jmax(CB_THRESHOLD.min, p.cb_threshold - 1.0);
            p.concurrency_limit = jmax(CONCURRENCY.min, (p.concurrency_limit * 0.7).floor());
        } else if er < 0.1 {
            p.retry_count = jmin(RETRY.max, p.retry_count + 1.0);
            p.cb_threshold = jmin(CB_THRESHOLD.max, p.cb_threshold + 1.0);
            p.concurrency_limit = jmin(CONCURRENCY.max, (p.concurrency_limit * 1.2).floor());
        }
        let p90 = f(local, "p90");
        if p90 > 3000.0 {
            p.timeout_ms = jmax(TIMEOUT.min, p.timeout_ms - 500.0);
            p.backoff_multiplier = jmin(BACKOFF.max, p.backoff_multiplier + 0.5);
        } else if p90 < 500.0 {
            p.timeout_ms = jmin(TIMEOUT.max, p.timeout_ms + 500.0);
            p.backoff_multiplier = jmax(BACKOFF.min, p.backoff_multiplier - 0.5);
        }
        if all.iter().any(|(id, s)| id != service_id && f(s, "errorRate") > 0.3) {
            p.concurrency_limit = jmax(CONCURRENCY.min, (p.concurrency_limit * 0.8).floor());
            p.retry_count = jmax(RETRY.min, p.retry_count - 1.0);
        }
        let p = p.clamp();
        self.per_service.insert(service_id.to_string(), p);
        self.decision_history.push(json!({ "timestamp": now_ms, "serviceId": service_id, "decision": p.to_json() }));
        p
    }
}

pub struct CentralStrategy {
    pub params: Mutex<Params>,
    pub pipelines: Mutex<Pipelines>,
}

impl CentralStrategy {
    pub fn new() -> Self {
        CentralStrategy {
            params: Mutex::new(Params::default()),
            pipelines: Mutex::default(),
        }
    }
}

impl Default for CentralStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl ResilienceStrategy for CentralStrategy {
    fn call_downstreams<'a>(
        &'a self,
        client: &'a reqwest::Client,
        downstreams: &'a [Downstream],
    ) -> BoxFuture<'a, Result<Vec<Value>, String>> {
        Box::pin(async move {
            let calls = downstreams.iter().map(|ds| async move {
                let c = PipelineConfig::from_params(&self.params.lock().unwrap());
                let pl = self.pipelines.lock().unwrap().get_or_create(&ds.id, ds.id.clone(), c, now_ms());
                // re-read on every attempt, like JS `_fetch` (a /control update applies to retries)
                call_pipelined(client, pl, &ds.url, || self.params.lock().unwrap().timeout_ms).await
            });
            let results = join_all(calls).await;
            Ok(format_pipelined_results(downstreams, results))
        })
    }

    /// `/control` (central only): `{...currentParams, ...params}`, then update pipelines.
    fn apply_update(&self, body: &Value) {
        let mut p = self.params.lock().unwrap();
        let mut merged = p.to_json();
        if let (Some(m), Some(b)) = (merged.as_object_mut(), body.as_object()) {
            for (k, v) in b {
                if m.contains_key(k) && v.is_number() {
                    m.insert(k.clone(), v.clone());
                }
            }
        }
        *p = serde_json::from_value(merged).unwrap();
        self.pipelines.lock().unwrap().apply(&p);
    }

    fn stats(&self) -> Value {
        let mut m = Map::new();
        m.insert("strategy".into(), json!("central"));
        m.insert("currentParams".into(), self.params.lock().unwrap().to_json());
        self.pipelines.lock().unwrap().stats_into(&mut m);
        Value::Object(m)
    }

    fn reset(&self) {
        self.pipelines.lock().unwrap().clear();
        *self.params.lock().unwrap() = Params::default();
    }
}
