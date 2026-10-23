//! Retry-only resilience strategy (`retry-only`).
//!
//! Applies retry with exponential backoff on transient downstream failures,
//! without any circuit breaker or concurrency limiter.

use std::sync::{Arc, Mutex};

use futures::future::join_all;
use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use super::{BoxFuture, ResilienceStrategy, fetch_json, format_pipelined_results};
use crate::resilience::{RetryPolicy, RetryState, execute};
use crate::topology::Downstream;

#[derive(Clone, Copy, Debug)]
pub struct RetryOnlyConfig {
    pub retry_count: f64,
    pub retry_base_ms: f64,
    pub retry_backoff: f64,
    pub request_timeout_ms: f64,
}

impl Default for RetryOnlyConfig {
    fn default() -> Self {
        RetryOnlyConfig {
            retry_count: 3.0,
            retry_base_ms: 200.0,
            retry_backoff: 2.0,
            request_timeout_ms: 3000.0,
        }
    }
}

struct PolicyRetry(Arc<Mutex<RetryPolicy>>);

impl RetryState for PolicyRetry {
    fn with_policy<R>(&self, f: impl FnOnce(&mut RetryPolicy) -> R) -> R {
        f(&mut self.0.lock().unwrap())
    }
}

pub struct RetryOnlyStrategy {
    cfg: RetryOnlyConfig,
    policies: Mutex<IndexMap<String, Arc<Mutex<RetryPolicy>>>>,
}

impl RetryOnlyStrategy {
    pub fn new() -> Self {
        Self::with_config(RetryOnlyConfig::default())
    }

    pub fn with_config(cfg: RetryOnlyConfig) -> Self {
        RetryOnlyStrategy {
            cfg,
            policies: Mutex::default(),
        }
    }

    fn policy_for(&self, id: &str) -> Arc<Mutex<RetryPolicy>> {
        let mut map = self.policies.lock().unwrap();
        map.entry(id.to_string())
            .or_insert_with(|| {
                Arc::new(Mutex::new(RetryPolicy::new(
                    self.cfg.retry_count,
                    self.cfg.retry_base_ms,
                    self.cfg.retry_backoff,
                )))
            })
            .clone()
    }
}

impl Default for RetryOnlyStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl ResilienceStrategy for RetryOnlyStrategy {
    fn call_downstreams<'a>(
        &'a self,
        client: &'a reqwest::Client,
        downstreams: &'a [Downstream],
    ) -> BoxFuture<'a, Result<Vec<Value>, String>> {
        Box::pin(async move {
            let calls = downstreams.iter().map(|ds| async move {
                let policy = self.policy_for(&ds.id);
                let t = self.cfg.request_timeout_ms;
                execute(&PolicyRetry(policy), |_attempt| {
                    let url = &ds.url;
                    async move { fetch_json(client, url, t).await }
                })
                .await
            });
            let results = join_all(calls).await;
            Ok(format_pipelined_results(downstreams, results))
        })
    }

    fn stats(&self) -> Value {
        let mut m = Map::new();
        m.insert("strategy".into(), json!("retry-only"));
        let mut pol_map = Map::new();
        let map = self.policies.lock().unwrap();
        for (id, p) in map.iter() {
            pol_map.insert(id.clone(), p.lock().unwrap().stats());
        }
        m.insert("retryPolicies".into(), Value::Object(pol_map));
        Value::Object(m)
    }

    fn reset(&self) {
        let map = self.policies.lock().unwrap();
        for p in map.values() {
            p.lock().unwrap().reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_shape_and_reset() {
        let strategy = RetryOnlyStrategy::new();
        let p = strategy.policy_for("svc-01");
        {
            let mut pol = p.lock().unwrap();
            pol.total_attempts = 5;
            pol.total_retries = 3;
        }

        let st = strategy.stats();
        assert_eq!(st["strategy"], "retry-only");
        assert_eq!(st["retryPolicies"]["svc-01"]["totalAttempts"], 5);
        assert_eq!(st["retryPolicies"]["svc-01"]["totalRetries"], 3);

        strategy.reset();
        let st2 = strategy.stats();
        assert_eq!(st2["retryPolicies"]["svc-01"]["totalAttempts"], 0);
        assert_eq!(st2["retryPolicies"]["svc-01"]["totalRetries"], 0);
    }
}
