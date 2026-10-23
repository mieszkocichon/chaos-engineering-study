//! The experiment service – port of `lib/generic-service.js` (HTTP contract:
//! specs/001-rust-rewrite/contracts/service-http.md).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum::body::Bytes;
use serde_json::{Value, json};

use crate::faults::FaultInjector;
use crate::metrics::MetricsCollector;
use crate::prng::Prng;
use crate::strategy::Strategy;
use crate::topology::{Downstream, truthy};
use crate::now_ms;

pub struct AppState {
    pub id: String,
    pub downstreams: Vec<Downstream>,
    pub processing_delay: Value,
    pub metrics: Mutex<MetricsCollector>,
    pub faults: FaultInjector,
    pub strategy: Option<Arc<Strategy>>,
    pub client: reqwest::Client,
    seed: Mutex<Value>,
    prng: Mutex<Prng>,
    started: Instant,
}

/// `String(seed)` for the PRNG seed string.
fn seed_str(seed: &Value) -> String {
    match seed {
        Value::String(s) => s.clone(),
        Value::Number(n) => metastable::js::js_number(n.as_f64().unwrap()),
        other => other.to_string(),
    }
}

fn delay_prng(seed: &Value, id: &str) -> Prng {
    Prng::new(&format!("{}_{id}_delay", seed_str(seed)))
}

impl AppState {
    pub fn new(id: &str, downstreams: Vec<Downstream>, processing_delay: Value, seed: Value, strategy: Option<Arc<Strategy>>) -> Arc<Self> {
        Arc::new(AppState {
            id: id.into(),
            downstreams,
            processing_delay,
            metrics: Mutex::new(MetricsCollector::default()),
            faults: FaultInjector::new(),
            strategy,
            client: reqwest::Client::new(),
            prng: Mutex::new(delay_prng(&seed, id)),
            seed: Mutex::new(seed),
            started: Instant::now(),
        })
    }

    /// `getProcessingDelay()` in ms.
    pub fn processing_delay_ms(&self) -> f64 {
        let d = &self.processing_delay;
        if let Some(n) = d.as_f64() {
            return n;
        }
        let field = |k: &str, def: f64| d.get(k).filter(|v| truthy(v)).and_then(Value::as_f64).unwrap_or(def);
        match d.get("distribution").and_then(Value::as_str) {
            Some("normal") => {
                let v = self.prng.lock().unwrap().normal_random(field("mean", 50.0), field("std", 10.0));
                metastable::js::js_round(v).max(0.0)
            }
            Some("uniform") => self.prng.lock().unwrap().uniform(field("min", 10.0), field("max", 100.0)),
            _ => field("mean", 50.0),
        }
    }

    pub fn uptime(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// Soft reset between seeds (`/experiment/reset`).
    pub async fn soft_reset(&self, new_seed: Option<Value>) {
        let seed = {
            let mut s = self.seed.lock().unwrap();
            if let Some(n) = new_seed.filter(|v| !v.is_null()) {
                *s = n;
            }
            s.clone()
        };
        *self.prng.lock().unwrap() = delay_prng(&seed, &self.id);
        self.metrics.lock().unwrap().reset();
        self.faults.recover().await;
        if let Some(s) = &self.strategy {
            s.reset();
        }
    }
}

/// Lenient JSON body: like `express.json()` + `req.body || {}`.
fn body_json(b: &Bytes) -> Value {
    serde_json::from_slice::<Value>(b).ok().filter(|v| !v.is_null()).unwrap_or_else(|| json!({}))
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/data", get(data))
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/gossip", post(gossip))
        .route("/control", post(control))
        .route("/faults/recover", post(recover))
        .route("/faults/recover/{kind}", post(recover_type))
        .route("/faults/{profile}", post(inject))
        .route("/start", post(start))
        .route("/experiment/reset", post(reset))
        .with_state(state)
}

async fn data(State(st): State<Arc<AppState>>) -> Response {
    let start = now_ms();
    st.metrics.lock().unwrap().start_request();
    let delay = st.processing_delay_ms();
    if delay > 0.0 {
        tokio::time::sleep(Duration::from_secs_f64(delay / 1000.0)).await;
    }
    let mut downstream_results = Value::Null;
    let mut failure = None;
    if !st.downstreams.is_empty() {
        if let Some(s) = &st.strategy {
            match s.call_downstreams(&st.client, &st.downstreams).await {
                Ok(v) => downstream_results = Value::Array(v),
                Err(e) => failure = Some(e),
            }
        }
    }
    let end = now_ms();
    let latency = end.saturating_sub(start);
    st.metrics.lock().unwrap().record_request(end, latency as f64, failure.is_none());
    match failure {
        None => Json(json!({
            "service": st.id, "latencyMs": latency, "downstreamResults": downstream_results, "timestamp": now_ms(),
        }))
        .into_response(),
        Some(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": e, "service": st.id }))).into_response(),
    }
}

async fn health(State(st): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({ "status": "ok", "service": st.id, "uptime": st.uptime() }))
}

/// (resident MB, user µs, system µs) – stand-ins for `process.memoryUsage()` / `cpuUsage()`.
fn resources() -> (u64, u64, u64) {
    let rss_pages = std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()))
        .unwrap_or(0);
    // SAFETY: getrusage writes into the zeroed struct we own.
    let ru = unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        ru
    };
    let us = |t: libc::timeval| t.tv_sec as u64 * 1_000_000 + t.tv_usec as u64;
    (rss_pages * 4096 / 1024 / 1024, us(ru.ru_utime), us(ru.ru_stime))
}

async fn metrics(State(st): State<Arc<AppState>>) -> Json<Value> {
    let now = now_ms();
    let (snapshot, health) = {
        let m = st.metrics.lock().unwrap();
        (m.snapshot(now), m.health_summary(now))
    };
    let (mem, cpu_user, cpu_system) = resources();
    Json(json!({
        "service": st.id,
        "snapshot": snapshot,
        "resources": { "memoryMb": mem, "cpuUser": cpu_user, "cpuSystem": cpu_system, "uptime": st.uptime() },
        "healthSummary": health,
        "strategyStats": st.strategy.as_ref().map(|s| s.stats()),
        "timestamp": now_ms(),
    }))
}

async fn gossip(State(st): State<Arc<AppState>>, body: Bytes) -> Json<Value> {
    if let Some(s) = st.strategy.as_ref().filter(|s| s.has_gossip_receiver()) {
        s.receive_gossip(&body_json(&body));
    }
    Json(json!({ "ok": true }))
}

async fn control(State(st): State<Arc<AppState>>, body: Bytes) -> Json<Value> {
    if let Some(s) = &st.strategy {
        s.apply_update(&body_json(&body));
    }
    Json(json!({ "ok": true }))
}

async fn recover(State(st): State<Arc<AppState>>) -> Json<Value> {
    st.faults.recover().await;
    Json(json!({ "status": "recovered" }))
}

async fn recover_type(State(st): State<Arc<AppState>>, Path(kind): Path<String>) -> Json<Value> {
    st.faults.recover_by_type(&kind).await;
    Json(json!({ "status": "recovered", "type": kind }))
}

async fn inject(State(st): State<Arc<AppState>>, Path(profile): Path<String>, body: Bytes) -> Json<Value> {
    st.faults.inject(&profile, &body_json(&body)).await;
    Json(json!({ "status": "injected", "profile": profile }))
}

async fn start(State(st): State<Arc<AppState>>, body: Bytes) -> Json<Value> {
    let b = body_json(&body);
    let t = b.get("experimentStartTime").filter(|v| v.is_number()).cloned().unwrap_or_else(|| json!(now_ms()));
    Json(json!({ "ok": true, "experimentStartTime": t, "service": st.id }))
}

async fn reset(State(st): State<Arc<AppState>>, body: Bytes) -> Json<Value> {
    let b = body_json(&body);
    st.soft_reset(b.get("seed").cloned()).await;
    let seed = st.seed.lock().unwrap().clone();
    Json(json!({ "ok": true, "service": st.id, "seed": seed }))
}
