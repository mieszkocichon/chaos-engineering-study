//! Resilience strategies (`STRATEGY` env).
//!
//! Open/Closed architecture: each strategy is a plugin implementing [`ResilienceStrategy`],
//! registered in [`create_strategy`]. Common pipeline and HTTP execution helpers are shared.

pub mod backpressure;
pub mod central;
pub mod mas;
pub mod mas_strategy;
pub mod none;
pub mod pipeline;
pub mod r#static;
pub mod retry_only;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

pub use self::r#static::StaticConfig;
use self::mas_strategy::MasStrategy;
use self::mas::GossipIn;
use self::pipeline::SharedPipeline;
use crate::metrics::{HealthSummary, Snapshot};
use crate::now_ms;
use crate::resilience::{RetryPolicy, RetryState, execute};
use crate::topology::{Downstream, Service};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Resilience strategy plugin trait.
pub trait ResilienceStrategy: Send + Sync {
    /// Execute downstream calls (`callDownstreams`).
    ///
    /// For most strategies, errors return degraded JSON objects (200 OK from service).
    /// For `none`, downstream errors bubble up as `Err` causing HTTP 502 Bad Gateway.
    fn call_downstreams<'a>(
        &'a self,
        client: &'a reqwest::Client,
        downstreams: &'a [Downstream],
    ) -> BoxFuture<'a, Result<Vec<Value>, String>>;

    /// Whether this strategy listens for gossip messages on `/gossip`.
    fn has_gossip_receiver(&self) -> bool {
        false
    }

    /// Handle an incoming gossip message on `/gossip`.
    fn receive_gossip(&self, _body: &Value) {}

    /// Apply dynamic parameters received on `/control` (centralized controller).
    fn apply_update(&self, _body: &Value) {}

    /// Strategy statistics for `/metrics` (`strategyStats` field).
    fn stats(&self) -> Value;

    /// Soft reset between seeds (`/experiment/reset`).
    fn reset(&self) {}

    /// Spawn background timers (`start()`): MAS decide/gossip, backpressure gossip.
    fn start(
        self: Arc<Self>,
        _client: reqwest::Client,
        _health: Arc<dyn Fn() -> HealthSummary + Send + Sync>,
        _snapshot: Arc<dyn Fn() -> Snapshot + Send + Sync>,
    ) {
    }
}

/// What services hold: any strategy plugin behind a trait object.
pub type Strategy = dyn ResilienceStrategy;

/// Names accepted by [`create_strategy`] (experiment validation); keep in sync with its `match`.
pub const STRATEGY_NAMES: &[&str] = &[
    "none", "static", "static-conservative", "central", "mas-agent", "mas-agent-swim",
    "mas-agent-inbound", "backpressure", "retry-only",
];

/// Factory registry: `STRATEGY` env name -> plugin constructor.
pub fn create_strategy(
    name: &str,
    svc: &Service,
    static_params: Option<&str>,
    swim_fanout: &str,
) -> Result<Option<Arc<dyn ResilienceStrategy>>, String> {
    if svc.downstreams.is_empty() {
        return Ok(None);
    }
    let strategy: Arc<dyn ResilienceStrategy> = match name {
        "none" => Arc::new(none::NoneStrategy::new(3000.0)),
        "static" => Arc::new(r#static::StaticStrategy::from_static_params(static_params)),
        "static-conservative" => Arc::new(r#static::StaticStrategy::new("static-conservative", StaticConfig::conservative())),
        "central" => Arc::new(central::CentralStrategy::new()),
        "mas-agent" => Arc::new(MasStrategy::new(&svc.id, svc.neighbors.clone(), mas_strategy::Plain)),
        "mas-agent-swim" => Arc::new(MasStrategy::new(&svc.id, svc.neighbors.clone(), mas_strategy::Swim::new(&svc.id, swim_fanout))),
        "mas-agent-inbound" => Arc::new(MasStrategy::new(&svc.id, svc.neighbors.clone(), mas_strategy::InboundReports::default())),
        "backpressure" => Arc::new(backpressure::BackpressureStrategy::new(&svc.id, svc.neighbors.clone())),
        "retry-only" => Arc::new(retry_only::RetryOnlyStrategy::new()),
        other => return Err(format!("[service-runner] Unknown strategy: {other}")),
    };
    Ok(Some(strategy))
}

/// `fetch(url)` with an abort timeout; errors use Node/undici wording, because they end up
/// in `downstreamResults` and in 502 bodies.
pub async fn fetch_json(client: &reqwest::Client, url: &str, timeout_ms: f64) -> Result<Value, String> {
    let res = client
        .get(url)
        .timeout(Duration::from_secs_f64(timeout_ms.max(1.0) / 1000.0))
        .send()
        .await
        .map_err(node_error)?;
    let status = res.status();
    let text = res.text().await.map_err(node_error)?;
    if !status.is_success() {
        return Err(format!("HTTP {}: {text}", status.as_u16()));
    }
    serde_json::from_str(&text).map_err(|e| format!("Unexpected token in JSON: {e}"))
}

pub fn node_error(e: reqwest::Error) -> String {
    if e.is_timeout() { "This operation was aborted".into() } else { "fetch failed".into() }
}

struct PipelineRetry<'a>(&'a SharedPipeline);
impl RetryState for PipelineRetry<'_> {
    fn with_policy<R>(&self, f: impl FnOnce(&mut RetryPolicy) -> R) -> R {
        f(&mut self.0.lock().unwrap().retrier)
    }
}

/// limiter → retry → breaker → fetch for one downstream (`_callOne`).
pub async fn call_pipelined(
    client: &reqwest::Client,
    pl: SharedPipeline,
    url: &str,
    timeout: impl Fn() -> f64,
) -> Result<Value, String> {
    pl.lock().unwrap().limiter.acquire()?;
    let r = execute(&PipelineRetry(&pl), |_| {
        let pl = pl.clone();
        let t = timeout();
        async move {
            pl.lock().unwrap().breaker.before_call(now_ms())?;
            let r = fetch_json(client, url, t).await;
            let mut g = pl.lock().unwrap();
            match &r {
                Ok(_) => g.breaker.on_success(now_ms()),
                Err(_) => g.breaker.on_failure(now_ms()),
            }
            r
        }
    })
    .await;
    pl.lock().unwrap().limiter.release();
    r
}

/// Helper for pipelined downstream calls: formats failed calls as degraded results.
pub fn format_pipelined_results(
    downstreams: &[Downstream],
    results: Vec<Result<Value, String>>,
) -> Vec<Value> {
    results
        .into_iter()
        .zip(downstreams)
        .map(|(r, ds)| match r {
            Ok(v) => v,
            Err(msg) => json!({ "error": msg, "service": ds.id, "degraded": true }),
        })
        .collect()
}

/// Parses standard gossip payload from HTTP body.
///
/// Quirk Q1: `inboundReport` is omitted from HTTP gossip receiver.
pub fn parse_gossip_in(body: &Value) -> Option<(String, GossipIn)> {
    let from = body.get("fromId").filter(|v| crate::topology::truthy(v))?;
    let from = from.as_str().map(String::from).unwrap_or_else(|| from.to_string());
    let f = |k: &str| body.get(k).and_then(Value::as_f64);
    let g = GossipIn {
        error_rate: f("errorRate"),
        avg_latency: f("avgLatency"),
        load: f("load"),
        timestamp: f("timestamp"),
        inbound_report: None,
    };
    Some((from, g))
}

/// `setInterval(f, ms)` – first run after one period, like JS.
pub fn spawn_every<F, Fut>(ms: u64, mut f: F) -> tokio::task::JoinHandle<()>
where
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(ms));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        tick.tick().await;
        loop {
            tick.tick().await;
            // timers do not wait for the previous run (setInterval semantics)
            tokio::spawn(f());
        }
    })
}
