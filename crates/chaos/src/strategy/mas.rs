//! MAS agent and its SWIM / Inbound variants – ports of `lib/strategies/mas-agent*.js`.
//! Pure decision logic; the clock is passed in (research R8).

use indexmap::IndexMap;
use metastable::js::js_fixed_num;
use serde_json::{Map, Value, json};

use super::pipeline::{PipelineConfig, Pipelines, SharedPipeline};
use crate::metrics::{HealthSummary, Snapshot};
use crate::resilience::*;
use crate::topology::Neighbor;

/// Neighbor health as received by gossip; missing fields behave like JS `undefined`.
#[derive(Clone, Copy, Debug, Default)]
pub struct GossipState {
    pub error_rate: Option<f64>,
    pub avg_latency: Option<f64>,
    pub load: Option<f64>,
    pub timestamp: f64,
}

/// What the HTTP `/gossip` receiver hands to the agent.
#[derive(Clone, Debug, Default)]
pub struct GossipIn {
    pub error_rate: Option<f64>,
    pub avg_latency: Option<f64>,
    pub load: Option<f64>,
    pub timestamp: Option<f64>,
    /// Only set by direct calls: `createGossipReceiver` drops it (quirk Q1).
    pub inbound_report: Option<InboundReport>,
}

#[derive(Clone, Debug)]
pub struct InboundReport {
    pub target: String,
    pub drop_rate: f64,
}

/// `x > y` with JS semantics for `undefined` (always false).
fn gt(x: Option<f64>, y: f64) -> bool {
    x.is_some_and(|x| x > y)
}

pub struct MasAgent {
    pub service_id: String,
    pub neighbors: Vec<Neighbor>,
    pub decision_interval_ms: u64,
    pub gossip_interval_ms: u64,
    pub params: Params,
    pub neighbor_state: IndexMap<String, GossipState>,
    pub gossip_stale_ms: f64,
    pub pipelines: Pipelines,
    pub decision_history: Vec<Value>,
    prev_p90: f64,
}

pub struct Observation {
    pub local: Snapshot,
    pub neighbors: Vec<GossipState>,
    pub open_breakers: Vec<String>,
}

impl MasAgent {
    pub fn new(service_id: &str, neighbors: Vec<Neighbor>) -> Self {
        MasAgent {
            service_id: service_id.into(),
            neighbors,
            decision_interval_ms: 2000,
            gossip_interval_ms: 3000,
            params: Params::default(),
            neighbor_state: IndexMap::new(),
            gossip_stale_ms: 9000.0,
            pipelines: Pipelines::default(),
            decision_history: Vec::new(),
            prev_p90: 0.0,
        }
    }

    pub fn get_or_create(&mut self, ds: &str, now_ms: u64) -> SharedPipeline {
        let name = format!("{}->{ds}", self.service_id);
        let c = PipelineConfig::from_params(&self.params);
        self.pipelines.get_or_create(ds, name, c, now_ms)
    }

    pub fn observe(&self, local: Snapshot, now_ms: u64) -> Observation {
        let open_breakers =
            self.pipelines.0.iter().filter(|(_, p)| p.lock().unwrap().breaker.state == CbState::Open).map(|(id, _)| id.clone()).collect();
        let neighbors =
            self.neighbor_state.values().filter(|d| (now_ms as f64 - d.timestamp) < self.gossip_stale_ms).copied().collect();
        Observation { local, neighbors, open_breakers }
    }

    pub fn decide(&mut self, obs: &Observation) -> Params {
        let local = &obs.local;
        let mut p = self.params;
        let open_count = obs.open_breakers.len();
        let total = self.pipelines.0.len();

        if open_count > 0 && total > 0 {
            let degraded = open_count as f64 / total as f64;
            p.retry_count = jmax(RETRY.min, p.retry_count - 1.0);
            p.cb_threshold = jmax(CB_THRESHOLD.min, p.cb_threshold - 1.0);
            p.concurrency_limit = jmax(CONCURRENCY.min, (p.concurrency_limit * (1.0 - jmin(degraded, 0.4))).floor());
        } else if open_count == 0 && total > 0 && local.request_count > 5 {
            p.retry_count = jmin(RETRY.max, p.retry_count + 1.0);
            p.cb_threshold = jmin(CB_THRESHOLD.max, p.cb_threshold + 1.0);
            p.concurrency_limit = jmin(CONCURRENCY.max, (p.concurrency_limit * 1.15).floor());
        }

        // trend matters here — rising p90 with no open breakers means congestion, not failure
        let trend = local.p90 - self.prev_p90;
        self.prev_p90 = local.p90;
        if local.p90 > 3000.0 {
            if trend > 150.0 && open_count == 0 {
                p.timeout_ms = jmin(TIMEOUT.max, p.timeout_ms + 1000.0);
                p.backoff_multiplier = jmax(BACKOFF.min, p.backoff_multiplier - 0.5);
            } else {
                p.timeout_ms = jmax(TIMEOUT.min, p.timeout_ms - 500.0);
                p.backoff_multiplier = jmin(BACKOFF.max, p.backoff_multiplier + 0.5);
            }
        } else if local.p90 < 500.0 && local.request_count > 5 {
            p.timeout_ms = jmin(TIMEOUT.max, p.timeout_ms + 500.0);
            p.backoff_multiplier = jmax(BACKOFF.min, p.backoff_multiplier - 0.5);
        }

        if obs.neighbors.iter().any(|n| gt(n.error_rate, 0.3)) {
            p.concurrency_limit = jmax(CONCURRENCY.min, (p.concurrency_limit * 0.8).floor());
            p.retry_count = jmax(RETRY.min, p.retry_count - 1.0);
        }
        p.clamp()
    }

    pub fn act(&mut self, p: Params) {
        self.params = p;
        self.pipelines.apply(&p);
    }

    /// `_tick()`: observe → decide → record → act.
    pub fn tick(&mut self, local: Snapshot, now_ms: u64) {
        let obs = self.observe(local, now_ms);
        let decision = self.decide(&obs);
        self.decision_history.push(json!({
            "timestamp": now_ms,
            "observation": {
                "errorRate": obs.local.error_rate,
                "p90": obs.local.p90,
                "requestCount": obs.local.request_count,
                "queueDepth": obs.local.queue_depth,
                "neighborCount": obs.neighbors.len(),
                "openBreakers": obs.open_breakers,
            },
            "decision": decision.to_json(),
        }));
        self.act(decision);
    }

    /// `receiveGossip`: timestamp falls back to now when missing or 0 (`||`).
    pub fn receive_gossip(&mut self, from: &str, g: &GossipIn, now_ms: u64) {
        let ts = g.timestamp.filter(|t| *t != 0.0 && !t.is_nan()).unwrap_or(now_ms as f64);
        self.neighbor_state.insert(
            from.to_string(),
            GossipState { error_rate: g.error_rate, avg_latency: g.avg_latency, load: g.load, timestamp: ts },
        );
    }

    pub fn stats(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("serviceId".into(), json!(self.service_id));
        m.insert("currentParams".into(), self.params.to_json());
        m.insert("decisionCount".into(), json!(self.decision_history.len()));
        m.insert("neighborCount".into(), json!(self.neighbor_state.len()));
        m.insert("history".into(), Value::Array(self.decision_history.clone()));
        m
    }

    pub fn reset(&mut self) {
        self.neighbor_state.clear();
        self.pipelines.clear();
        self.decision_history.clear();
        self.prev_p90 = 0.0;
        self.params = Params::default();
    }
}

// ------------------------------------------------------------------- SWIM

/// Seeded peer sampler of `mas-agent-swim.js` (`pickK` + mulberry32 seeded from the id).
pub struct SwimPicker {
    pub fanout: usize,
    rng: Box<dyn FnMut() -> f64 + Send>,
}

impl SwimPicker {
    pub fn new(service_id: &str, fanout: usize) -> Self {
        let sum: i64 = service_id.encode_utf16().map(i64::from).sum();
        let seed = (sum as i32) ^ 0x00C0_FFEE;
        SwimPicker { fanout: fanout.max(1), rng: Box::new(metastable::sim::mulberry32(seed as f64)) }
    }

    pub fn pick<'a, T>(&mut self, arr: &'a [T]) -> Vec<&'a T> {
        if arr.len() <= self.fanout {
            return arr.iter().collect();
        }
        let mut copy: Vec<&T> = arr.iter().collect();
        (0..self.fanout).map(|_| copy.remove(((self.rng)() * copy.len() as f64).floor() as usize)).collect()
    }
}

// ------------------------------------------------------------------- Inbound

const INBOUND_DROP_THRESHOLD: f64 = 0.3;
const INBOUND_MIN_SAMPLES: u64 = 5;
const INBOUND_WINDOW_MS: f64 = 8000.0;
const INBOUND_REPORT_TTL_MS: f64 = 10_000.0;

#[derive(Clone, Copy, Debug)]
struct DsCounter {
    ok: u64,
    fail: u64,
    window_start: f64,
}

#[derive(Clone, Debug)]
struct SelfReport {
    reporter_id: String,
    drop_rate: f64,
    timestamp: f64,
}

#[derive(Default)]
pub struct Inbound {
    counters: IndexMap<String, DsCounter>,
    self_report: Option<SelfReport>,
    reports_emitted: u64,
    reports_received: u64,
    self_inbound_inflations: u64,
    /// JS lazily adds a `selfInflations` key on first inflation – kept for identical stats.
    self_inflations: Option<u64>,
}

impl Inbound {
    pub fn record_downstream_result(&mut self, ds: &str, ok: bool, now_ms: u64) {
        let now = now_ms as f64;
        let c = self.counters.entry(ds.to_string()).or_insert(DsCounter { ok: 0, fail: 0, window_start: now });
        if now - c.window_start > INBOUND_WINDOW_MS {
            *c = DsCounter { ok: 0, fail: 0, window_start: now };
        }
        if ok {
            c.ok += 1;
        } else {
            c.fail += 1;
        }
    }

    fn fail_rate(&self, ds: &str) -> (f64, u64) {
        match self.counters.get(ds) {
            None => (0.0, 0),
            Some(c) => {
                let samples = c.ok + c.fail;
                (if samples == 0 { 0.0 } else { c.fail as f64 / samples as f64 }, samples)
            }
        }
    }

    /// Extra handling after the base `receiveGossip`.
    pub fn receive_gossip(&mut self, self_id: &str, from: &str, g: &GossipIn, now_ms: u64) {
        if let Some(r) = &g.inbound_report {
            if r.target == self_id {
                self.self_report = Some(SelfReport { reporter_id: from.into(), drop_rate: r.drop_rate, timestamp: now_ms as f64 });
                self.reports_received += 1;
            }
        }
    }

    /// Payloads of one gossip round: `(neighbor, payload)` (before `fromId`/`timestamp` are added).
    pub fn gossip_payloads(&mut self, neighbors: &[Neighbor], base: &HealthSummary, now_ms: u64) -> Vec<(Neighbor, Map<String, Value>)> {
        let mut summary = Map::new();
        summary.insert("errorRate".into(), json!(base.error_rate));
        summary.insert("avgLatency".into(), json!(base.avg_latency));
        summary.insert("load".into(), json!(base.load));
        if let Some(r) = &self.self_report {
            if now_ms as f64 - r.timestamp < INBOUND_REPORT_TTL_MS {
                summary.insert("errorRate".into(), json!(jmax(base.error_rate, r.drop_rate)));
                summary.insert("inboundInflatedBy".into(), json!(r.reporter_id));
                self.self_inflations = Some(self.self_inflations.unwrap_or(0) + 1);
                self.self_inbound_inflations += 1;
            }
        }
        neighbors
            .iter()
            .map(|n| {
                let (rate, samples) = self.fail_rate(&n.id);
                let mut payload = summary.clone();
                if samples >= INBOUND_MIN_SAMPLES && rate >= INBOUND_DROP_THRESHOLD {
                    payload.insert(
                        "inboundReport".into(),
                        json!({ "target": n.id, "dropRate": js_fixed_num(rate, 4), "samples": samples }),
                    );
                    self.reports_emitted += 1;
                }
                (n.clone(), payload)
            })
            .collect()
    }

    pub fn stats(&self) -> Value {
        let mut m = Map::new();
        m.insert("reportsEmitted".into(), json!(self.reports_emitted));
        m.insert("reportsReceived".into(), json!(self.reports_received));
        m.insert("selfInboundInflations".into(), json!(self.self_inbound_inflations));
        if let Some(v) = self.self_inflations {
            m.insert("selfInflations".into(), json!(v));
        }
        let per: Map<String, Value> = self
            .counters
            .iter()
            .map(|(id, c)| (id.clone(), json!({ "ok": c.ok, "fail": c.fail, "rate": self.fail_rate(id).0 })))
            .collect();
        m.insert("perDownstream".into(), Value::Object(per));
        m.insert(
            "lastSelfReport".into(),
            self.self_report.as_ref().map_or(Value::Null, |r| {
                json!({ "reporterId": r.reporter_id, "dropRate": r.drop_rate, "timestamp": r.timestamp })
            }),
        );
        Value::Object(m)
    }

    pub fn reset(&mut self) {
        *self = Inbound::default();
    }
}
