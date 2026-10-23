//! Static resilience strategy – pipeline per downstream with fixed parameters.

use std::sync::Mutex;

use futures::future::join_all;
use serde_json::{Map, Value, json};

use super::pipeline::{PipelineConfig, Pipelines};
use super::{BoxFuture, ResilienceStrategy, call_pipelined, format_pipelined_results};
use crate::now_ms;
use crate::topology::Downstream;

#[derive(Clone, Copy, Debug)]
pub struct StaticConfig {
    pub pipeline: PipelineConfig,
    pub request_timeout_ms: f64,
}

impl StaticConfig {
    /// `new StaticStrategy(opts)` defaults, overridable through `STATIC_PARAMS` keys.
    pub fn from_opts(o: &Value) -> Self {
        let n = |k: &str, d: f64| o.get(k).and_then(Value::as_f64).unwrap_or(d);
        StaticConfig {
            pipeline: PipelineConfig {
                retry_count: n("retryCount", 3.0),
                retry_base_ms: n("retryBaseMs", 200.0),
                retry_backoff: n("retryBackoff", 2.0),
                cb_threshold: n("cbThreshold", 5.0),
                cb_timeout_ms: n("cbTimeoutMs", 10_000.0),
                max_concurrent: n("maxConcurrent", 50.0),
            },
            request_timeout_ms: n("requestTimeoutMs", 3000.0),
        }
    }

    pub fn conservative() -> Self {
        Self::from_opts(&json!({
            "retryCount": 1,
            "cbThreshold": 15,
            "cbTimeoutMs": 5000,
            "maxConcurrent": 100,
            "retryBaseMs": 500,
        }))
    }
}

pub struct StaticStrategy {
    pub name: &'static str,
    pub cfg: StaticConfig,
    pub pipelines: Mutex<Pipelines>,
}

impl StaticStrategy {
    pub fn new(name: &'static str, cfg: StaticConfig) -> Self {
        StaticStrategy {
            name,
            cfg,
            pipelines: Mutex::default(),
        }
    }

    pub fn from_static_params(static_params: Option<&str>) -> Self {
        let opts = match static_params {
            Some(s) => serde_json::from_str(s).unwrap_or_else(|e| {
                eprintln!("[service-runner] bad STATIC_PARAMS: {e}");
                json!({})
            }),
            None => json!({}),
        };
        Self::new("static", StaticConfig::from_opts(&opts))
    }
}

impl ResilienceStrategy for StaticStrategy {
    fn call_downstreams<'a>(
        &'a self,
        client: &'a reqwest::Client,
        downstreams: &'a [Downstream],
    ) -> BoxFuture<'a, Result<Vec<Value>, String>> {
        Box::pin(async move {
            let calls = downstreams.iter().map(|ds| async move {
                let pl = self.pipelines.lock().unwrap().get_or_create(&ds.id, ds.id.clone(), self.cfg.pipeline, now_ms());
                let t = self.cfg.request_timeout_ms;
                call_pipelined(client, pl, &ds.url, || t).await
            });
            let results = join_all(calls).await;
            Ok(format_pipelined_results(downstreams, results))
        })
    }

    fn stats(&self) -> Value {
        let mut m = Map::new();
        m.insert("strategy".into(), json!(self.name));
        self.pipelines.lock().unwrap().stats_into(&mut m);
        Value::Object(m)
    }

    fn reset(&self) {
        self.pipelines.lock().unwrap().clear();
    }
}
