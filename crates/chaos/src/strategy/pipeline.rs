//! Per-downstream resilience pipeline (limiter → retry → circuit breaker), shared by the
//! static, central and MAS strategies (research R9).

use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use serde_json::{Value, json};

use crate::resilience::{CircuitBreaker, ConcurrencyLimiter, Params, RetryPolicy};

#[derive(Clone, Debug)]
pub struct Pipeline {
    pub breaker: CircuitBreaker,
    pub retrier: RetryPolicy,
    pub limiter: ConcurrencyLimiter,
}

/// Construction parameters of new pipelines.
#[derive(Clone, Copy, Debug)]
pub struct PipelineConfig {
    pub cb_threshold: f64,
    pub cb_timeout_ms: f64,
    pub retry_count: f64,
    pub retry_base_ms: f64,
    pub retry_backoff: f64,
    pub max_concurrent: f64,
}

impl PipelineConfig {
    pub fn from_params(p: &Params) -> Self {
        PipelineConfig {
            cb_threshold: p.cb_threshold,
            cb_timeout_ms: p.cb_timeout_ms,
            retry_count: p.retry_count,
            retry_base_ms: 200.0,
            retry_backoff: p.backoff_multiplier,
            max_concurrent: p.concurrency_limit,
        }
    }
}

pub type SharedPipeline = Arc<Mutex<Pipeline>>;

/// Insertion-ordered map downstream id → pipeline (JS `Map` order shows up in stats).
/// Pipelines are shared like JS objects: an in-flight call keeps using its pipeline even
/// if a soft reset clears the map.
#[derive(Default, Debug)]
pub struct Pipelines(pub IndexMap<String, SharedPipeline>);

impl Pipelines {
    pub fn get_or_create(&mut self, ds: &str, breaker_name: String, c: PipelineConfig, now_ms: u64) -> SharedPipeline {
        self.0
            .entry(ds.to_string())
            .or_insert_with(|| {
                Arc::new(Mutex::new(Pipeline {
                    breaker: CircuitBreaker::new(c.cb_threshold, c.cb_timeout_ms, breaker_name, now_ms),
                    retrier: RetryPolicy::new(c.retry_count, c.retry_base_ms, c.retry_backoff),
                    limiter: ConcurrencyLimiter::new(c.max_concurrent),
                }))
            })
            .clone()
    }

    /// `_act` / `applyUpdate`: push new params into every existing pipeline.
    pub fn apply(&mut self, p: &Params) {
        for pl in self.0.values() {
            let mut pl = pl.lock().unwrap();
            pl.breaker.update_params(Some(p.cb_threshold), Some(p.cb_timeout_ms));
            pl.retrier.update_params(Some(p.retry_count), Some(p.backoff_multiplier));
            pl.limiter.update_params(Some(p.concurrency_limit));
        }
    }

    /// Per-downstream stats entries `{circuitBreaker, retry, concurrency}`.
    pub fn stats_into(&self, out: &mut serde_json::Map<String, Value>) {
        for (id, pl) in &self.0 {
            let pl = pl.lock().unwrap();
            out.insert(
                id.clone(),
                json!({
                    "circuitBreaker": pl.breaker.stats(),
                    "retry": pl.retrier.stats(),
                    "concurrency": pl.limiter.stats(),
                }),
            );
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}
