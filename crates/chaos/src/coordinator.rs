//! Experiment-cell coordinator – port of `runner/coordinator.js`. One cluster lifecycle,
//! soft reset between seeds, faults scheduled relative to each seed's start.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::join_all;
use indexmap::IndexMap;
use metastable::js::{js_number, js_parse_int, js_round, js_to_fixed, to_js_json_pretty};
use serde_json::{Map, Value, json};

use crate::now_ms;
use crate::strategy::central::CentralDecider;
use crate::strategy::spawn_every;
use crate::topology::{Service, load_fault_profile, load_topology};

pub struct Config {
    pub topology: String,
    pub fault_base: String,
    pub fault_profile: String,
    pub fault_partition: String,
    pub strategy: String,
    pub seeds: Vec<i64>,
    pub rps: i64,
    pub duration_sec: i64,
    pub warmup_sec: i64,
    pub base_port: u16,
    pub results_dir: PathBuf,
    pub health_timeout_ms: u64,
    pub inter_seed_settle_ms: u64,
    pub spof_kill_at_sec: i64,
    pub k6_bin: String,
    pub k6_script: String,
    pub config_dir: PathBuf,
    pub host_override: Option<String>,
}

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string())
}

fn env_int(k: &str, d: &str) -> f64 {
    js_parse_int(&env_or(k, d)).unwrap_or(f64::NAN)
}

impl Config {
    /// Env contract: specs/001-rust-rewrite/contracts/cli-env.md.
    pub fn from_env() -> Result<Self, String> {
        let seeds: Vec<i64> = env_or("SEEDS", "1000,2000,3000,4000,5000")
            .split(',')
            .filter_map(|s| js_parse_int(s.trim()))
            .map(|v| v as i64)
            .collect();
        if seeds.is_empty() {
            return Err("[coordinator] SEEDS env var is empty or invalid".into());
        }
        Ok(Config {
            topology: env_or("TOPOLOGY", "ring-50"),
            fault_base: std::env::var("FAULT_BASE").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "no-wan".into()),
            fault_profile: env_or("FAULT_PROFILE", "baseline-flaky"),
            fault_partition: std::env::var("FAULT_PARTITION").unwrap_or_default(),
            strategy: env_or("STRATEGY", "none"),
            seeds,
            rps: env_int("RPS", "20") as i64,
            duration_sec: env_int("DURATION_SEC", "60") as i64,
            warmup_sec: env_int("WARMUP_SEC", "15") as i64,
            base_port: env_int("BASE_PORT", "4000") as u16,
            results_dir: PathBuf::from(env_or("RESULTS_DIR", "/results")),
            health_timeout_ms: env_int("HEALTH_TIMEOUT_MS", "300000") as u64,
            inter_seed_settle_ms: env_int("INTER_SEED_SETTLE_MS", "2000") as u64,
            spof_kill_at_sec: env_int("SPOF_KILL_AT_SEC", "0").max(0.0) as i64,
            k6_bin: env_or("K6_BIN", "k6"),
            k6_script: env_or("K6_SCRIPT", "/app/runner/k6-script.js"),
            config_dir: crate::topology::config_dir(),
            host_override: std::env::var("HOST_OVERRIDE").ok().filter(|v| !v.is_empty()),
        })
    }

    fn host(&self, id: &str) -> String {
        self.host_override.clone().unwrap_or_else(|| id.to_string())
    }
}

// ------------------------------------------------------------------ fault schedule

#[derive(Clone, Debug, PartialEq)]
pub struct FaultAction {
    /// delay from the moment the profile is applied
    pub at_ms: u64,
    pub service: String,
    pub url: String,
    /// `Some(options)` = inject, `None` = type-scoped recover
    pub body: Option<Value>,
}

/// `scheduleSingleFault` for every fault of a profile whose target exists. `options_for`
/// supplies the inject body (partition: options + resolved `peerIps`).
pub fn plan_faults(profile: &Value, services: &[Service], host: &dyn Fn(&str) -> String, options_for: &dyn Fn(&Value) -> Value) -> Vec<FaultAction> {
    let empty = vec![];
    let mut out = vec![];
    for fault in profile["faults"].as_array().unwrap_or(&empty) {
        let Some(svc) = services.iter().find(|s| Some(s.id.as_str()) == fault["targetService"].as_str()) else { continue };
        let kind = fault["type"].as_str().unwrap_or_default();
        let target = format!("http://{}:{}/faults", host(&svc.id), svc.port);
        let start = fault["startTimeSec"].as_f64().unwrap_or(0.0);
        out.push(FaultAction { at_ms: (start * 1000.0) as u64, service: svc.id.clone(), url: format!("{target}/{kind}"), body: Some(options_for(fault)) });

        let mut recover = fault.get("endTimeSec").and_then(Value::as_f64);
        if let Some(fault_kind) = crate::faults::find_fault_kind(kind) {
            if fault_kind.default_recover_after_sec().is_some() && !recover.is_some_and(|v| v != 0.0) {
                recover = fault_kind.default_recover_after_sec();
            }
        }
        if let Some(r) = recover.filter(|r| *r < 999.0) {
            out.push(FaultAction { at_ms: (r * 1000.0) as u64, service: svc.id.clone(), url: format!("{target}/recover/{kind}"), body: None });
        }
    }
    out
}

fn fault_options(fault: &Value) -> Value {
    fault.get("options").filter(|o| !o.is_null()).cloned().unwrap_or_else(|| json!({}))
}

// ------------------------------------------------------------------ result file

pub struct ResultMeta<'a> {
    pub cfg: &'a Config,
    pub seed: i64,
    pub experiment_start_time: u64,
    pub spof: Value,
    pub timestamp_iso: String,
}

/// `buildResult(...)`: summary derived from the k6 end-of-test summary (`k6Summary.metrics`).
pub fn build_result(k6_metrics: &Value, wall_clock_ms: u64, service_metrics: Value, m: &ResultMeta) -> Value {
    let vals = |name: &str| k6_metrics.get(name).and_then(|x| x.get("values")).cloned().unwrap_or_else(|| json!({}));
    let (dur, reqs, failed) = (vals("http_req_duration"), vals("http_reqs"), vals("http_req_failed"));
    // `x || 0` – missing or 0 → 0
    let f = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).filter(|x| *x != 0.0 && !x.is_nan());
    let total = js_round(f(&reqs, "count").unwrap_or(0.0));
    let failure = js_round(f(&failed, "rate").unwrap_or(0.0) * total);
    let success = total - failure;
    let c = m.cfg;
    json!({
        "config": {
            "topology": c.topology,
            "strategy": c.strategy,
            "faultBase": c.fault_base,
            "faultProfile": c.fault_profile,
            "faultPartition": if c.fault_partition.is_empty() { Value::Null } else { json!(c.fault_partition) },
            "seed": m.seed,
            "rps": c.rps,
            "durationSec": c.duration_sec,
            "warmupSec": c.warmup_sec,
            "softReset": true,
            "loadGenerator": "k6",
            "spof": m.spof,
        },
        "experimentStartTime": m.experiment_start_time,
        "summary": {
            "totalRequests": total as i64,
            "successCount": success as i64,
            "failureCount": failure as i64,
            "successRate": if total > 0.0 { success / total } else { 0.0 },
            "avgLatencyMs": js_round(f(&dur, "avg").unwrap_or(0.0)) as i64,
            "p50Ms": js_round(f(&dur, "p(50)").or_else(|| f(&dur, "med")).unwrap_or(0.0)) as i64,
            "p90Ms": js_round(f(&dur, "p(90)").unwrap_or(0.0)) as i64,
            "p99Ms": js_round(f(&dur, "p(99)").unwrap_or(0.0)) as i64,
            "minLatencyMs": js_round(f(&dur, "min").unwrap_or(0.0)) as i64,
            "maxLatencyMs": js_round(f(&dur, "max").unwrap_or(0.0)) as i64,
            "wallClockMs": wall_clock_ms,
        },
        "k6Metrics": k6_metrics,
        "serviceMetrics": service_metrics,
        "timestamp": m.timestamp_iso,
    })
}

pub fn result_filename(c: &Config, seed: i64, now: u64) -> PathBuf {
    let partition = if c.fault_partition.is_empty() { String::new() } else { format!("+{}", c.fault_partition) };
    c.results_dir.join(format!("{}_{}_{}_{}{partition}_{seed}_{now}.json", c.topology, c.strategy, c.fault_base, c.fault_profile))
}

/// `new Date(ms).toISOString()`.
pub fn iso8601(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(mo <= 2);
    format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3600, rem % 3600 / 60, rem % 60, ms % 1000)
}

// ------------------------------------------------------------------ cell run

async fn post(client: &reqwest::Client, url: &str, body: &Value, timeout_ms: u64) -> Result<(), String> {
    client.post(url).json(body).timeout(Duration::from_millis(timeout_ms)).send().await.map(|_| ()).map_err(|e| e.to_string())
}

async fn wait_for_healthy(client: &reqwest::Client, c: &Config, services: &[Service]) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_millis(c.health_timeout_ms);
    let mut pending: Vec<&Service> = services.iter().collect();
    println!("[coordinator] Waiting for {} services (timeout: {}ms)...", pending.len(), c.health_timeout_ms);
    while !pending.is_empty() && Instant::now() < deadline {
        let checks = pending.iter().map(|s| {
            let url = format!("http://{}:{}/health", c.host(&s.id), s.port);
            async move { client.get(url).timeout(Duration::from_millis(2000)).send().await.is_ok_and(|r| r.status().is_success()) }
        });
        let ok = join_all(checks).await;
        let (ready, still): (Vec<_>, Vec<_>) = pending.into_iter().zip(ok).partition(|(_, ok)| *ok);
        pending = still.into_iter().map(|(s, _)| s).collect();
        let mut remaining = pending.len() + ready.len();
        for (s, _) in ready {
            remaining -= 1;
            println!("[coordinator] {} healthy ({remaining} remaining)", s.id);
        }
        if !pending.is_empty() {
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    }
    if !pending.is_empty() {
        let ids: Vec<&str> = pending.iter().map(|s| s.id.as_str()).collect();
        return Err(format!("[coordinator] Health check timeout. Not ready: {}", ids.join(", ")));
    }
    println!("[coordinator] All services healthy");
    Ok(())
}

async fn broadcast(client: &reqwest::Client, c: &Config, services: &[Service], path: &str, body: Value, timeout_ms: u64) {
    join_all(services.iter().map(|s| {
        let url = format!("http://{}:{}{path}", c.host(&s.id), s.port);
        let body = body.clone();
        async move {
            if let Err(e) = post(client, &url, &body, timeout_ms).await {
                eprintln!("[coordinator] {} {path} failed: {e}", s.id);
            }
        }
    }))
    .await;
}

async fn collect_metrics(client: &reqwest::Client, c: &Config, services: &[Service]) -> Value {
    let rows = join_all(services.iter().map(|s| {
        let url = format!("http://{}:{}/metrics", c.host(&s.id), s.port);
        async move {
            let r: Result<Value, String> = async {
                let res = client.get(url).timeout(Duration::from_millis(5000)).send().await.map_err(|e| e.to_string())?;
                res.json::<Value>().await.map_err(|e| e.to_string())
            }
            .await;
            match r {
                Ok(data) => {
                    let mut m = data.get("snapshot").and_then(Value::as_object).cloned().unwrap_or_default();
                    m.insert("strategyStats".into(), data.get("strategyStats").cloned().unwrap_or(Value::Null));
                    (s.id.clone(), Value::Object(m))
                }
                Err(e) => {
                    eprintln!("[coordinator] metrics {}: {e}", s.id);
                    (s.id.clone(), Value::Null)
                }
            }
        }
    }))
    .await;
    Value::Object(rows.into_iter().collect::<Map<_, _>>())
}

/// Spawn the timers of one profile; peers are resolved via DNS if required by the fault.
async fn apply_profile(client: &reqwest::Client, c: &Config, services: &[Service], name: &str, experiment_start: u64) {
    let profile = match load_fault_profile(&c.config_dir, name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[coordinator] {e}");
            return;
        }
    };
    let mut peer_ips: IndexMap<String, Vec<String>> = IndexMap::new();
    for fault in profile["faults"].as_array().into_iter().flatten() {
        let kind = fault["type"].as_str().unwrap_or_default();
        let needs_peers = crate::faults::find_fault_kind(kind).is_some_and(|k| k.requires_peer_resolution());
        if needs_peers {
            let mut ips = vec![];
            for p in fault["peerServices"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                match tokio::net::lookup_host((c.host(p).as_str(), 0)).await {
                    Ok(mut a) => ips.extend(a.next().map(|a| a.ip().to_string())),
                    Err(e) => eprintln!("[coordinator] DNS lookup failed for {p}: {e}"),
                }
            }
            let target = fault["targetService"].as_str().unwrap_or_default().to_string();
            let peers: Vec<&str> = fault["peerServices"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            println!("[coordinator] Partition {target} ⇔ [{}] = [{}]", peers.join(","), ips.join(","));
            peer_ips.insert(target, ips);
        }
    }
    let options_for = |fault: &Value| {
        let mut o = fault_options(fault);
        let target = fault["targetService"].as_str().unwrap_or_default();
        if let Some(ips) = peer_ips.get(target) {
            if let Some(m) = o.as_object_mut() {
                m.insert("peerIps".into(), json!(ips));
            }
        }
        o
    };
    for a in plan_faults(&profile, services, &|id| c.host(id), &options_for) {
        let client = client.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(a.at_ms)).await;
            let t = (now_ms() as f64 - experiment_start as f64) / 1000.0;
            let kind = a.url.rsplit('/').next().unwrap_or_default().to_string();
            match &a.body {
                Some(body) => {
                    println!("[coordinator] [t={}s] Injecting '{kind}' on {}", js_to_fixed(t, 1), a.service);
                    if let Err(e) = post(&client, &a.url, body, 30_000).await {
                        eprintln!("[coordinator] Inject failed: {e}");
                    }
                }
                None => {
                    println!("[coordinator] [t={}s] Recovering '{kind}' on {}", js_to_fixed(t, 1), a.service);
                    if let Err(e) = post(&client, &a.url, &Value::Null, 30_000).await {
                        eprintln!("[coordinator] Recover failed: {e}");
                    }
                }
            }
        });
    }
}

/// The centralized controller loop for `STRATEGY=central` (poll /metrics, push /control).
fn start_central(client: reqwest::Client, c: &Config, services: &[Service]) -> tokio::task::JoinHandle<()> {
    let targets: Vec<(String, String, String)> = services
        .iter()
        .map(|s| {
            let base = format!("http://{}:{}", c.host(&s.id), s.port);
            (s.id.clone(), format!("{base}/metrics"), format!("{base}/control"))
        })
        .collect();
    let decider = Arc::new(std::sync::Mutex::new(CentralDecider::default()));
    spawn_every(2000, move || {
        let (client, targets, decider) = (client.clone(), targets.clone(), decider.clone());
        async move {
            let polls = targets.iter().map(|(id, metrics, _)| {
                let client = client.clone();
                async move {
                    let r = async {
                        let res = client.get(metrics).timeout(Duration::from_millis(1000)).send().await.ok()?;
                        res.json::<Value>().await.ok()
                    }
                    .await;
                    let snap = match r {
                        Some(d) => d.get("snapshot").filter(|v| crate::topology::truthy(v)).or_else(|| d.get("healthSummary").filter(|v| crate::topology::truthy(v))).cloned().unwrap_or_else(|| json!({})),
                        None => json!({ "errorRate": 1, "avgLatency": 99999, "unreachable": true }),
                    };
                    (id.clone(), snap)
                }
            });
            let snapshots: IndexMap<String, Value> = join_all(polls).await.into_iter().collect();
            for (id, _, control) in &targets {
                let p = decider.lock().unwrap().decide(id, &snapshots, now_ms());
                let (client, control, body) = (client.clone(), control.clone(), p.to_json());
                tokio::spawn(async move {
                    let _ = post(&client, &control, &body, 1000).await;
                });
            }
        }
    })
}

async fn run_k6(c: &Config, target_url: &str) -> Result<(Value, u64), String> {
    let summary_file = c.results_dir.join(format!("_k6_summary_tmp_{}.json", now_ms()));
    let start = Instant::now();
    println!(
        "[coordinator] k6: {} RPS | {}s warmup + {}s measurement | target={target_url}",
        c.rps, c.warmup_sec, c.duration_sec
    );
    let status = tokio::process::Command::new(&c.k6_bin)
        .args(["run", "--env", &format!("TARGET_URL={target_url}"), "--env", &format!("RPS={}", c.rps)])
        .args(["--env", &format!("DURATION_SEC={}", c.duration_sec), "--env", &format!("WARMUP_SEC={}", c.warmup_sec)])
        .args(["--env", &format!("SUMMARY_FILE={}", summary_file.display()), &c.k6_script])
        .status()
        .await
        .map_err(|e| format!("k6: {e}"))?;
    let wall = start.elapsed().as_millis() as u64;
    if !status.success() {
        return Err(format!("k6 exited with code {}", status.code().map_or("null".into(), |c| c.to_string())));
    }
    let raw = std::fs::read_to_string(&summary_file).map_err(|e| format!("Failed to parse k6 summary: {e}"))?;
    let _ = std::fs::remove_file(&summary_file);
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("Failed to parse k6 summary: {e}"))?;
    Ok((v, wall))
}

/// One experiment cell: health → (reset) → start → faults → [central] → k6 → metrics → file.
pub async fn run_cell(c: &Config) -> Result<Vec<PathBuf>, String> {
    let topo = load_topology(&c.config_dir, &c.topology, c.base_port, &|id| c.host(id))?;
    println!(
        "[coordinator] topology={} strategy={} base={} fault={} partition={} seeds=[{}]",
        c.topology,
        c.strategy,
        c.fault_base,
        c.fault_profile,
        if c.fault_partition.is_empty() { "(none)" } else { &c.fault_partition },
        c.seeds.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
    );
    let entry = topo.entry_point.clone().ok_or("[coordinator] topology has no entry point")?;
    println!("[coordinator] Services: {} (entry: {})", topo.services.len(), entry.id);
    let client = reqwest::Client::new();

    wait_for_healthy(&client, c, &topo.services).await?;
    std::fs::create_dir_all(&c.results_dir).map_err(|e| format!("{}: {e}", c.results_dir.display()))?;

    let mut written = vec![];
    for (i, &seed) in c.seeds.iter().enumerate() {
        println!("\n=== Cell seed {seed} ({}/{}) ===", i + 1, c.seeds.len());
        if i > 0 {
            println!("[coordinator] Soft reset — next seed={seed}");
            broadcast(&client, c, &topo.services, "/experiment/reset", json!({ "seed": seed }), 10_000).await;
            tokio::time::sleep(Duration::from_millis(c.inter_seed_settle_ms)).await;
        }
        let experiment_start = now_ms();
        println!("[coordinator] Broadcasting experimentStartTime={experiment_start}");
        broadcast(&client, c, &topo.services, "/start", json!({ "experimentStartTime": experiment_start }), 5000).await;

        if !(c.fault_base == "no-wan" || c.fault_base.is_empty()) {
            apply_profile(&client, c, &topo.services, &c.fault_base, experiment_start).await;
        }
        apply_profile(&client, c, &topo.services, &c.fault_profile, experiment_start).await;
        if !c.fault_partition.is_empty() {
            apply_profile(&client, c, &topo.services, &c.fault_partition, experiment_start).await;
        }

        let mut central = None;
        let killed_at = Arc::new(std::sync::Mutex::new(None::<u64>));
        let mut spof_timer = None;
        if c.strategy == "central" {
            central = Some(start_central(client.clone(), c, &topo.services));
            println!("[coordinator] Central controller started");
            if c.spof_kill_at_sec > 0 {
                let h = central.as_ref().unwrap().abort_handle();
                let (killed_at, kill_sec) = (killed_at.clone(), c.spof_kill_at_sec);
                let delay = ((c.warmup_sec + kill_sec) * 1000) as u64;
                spof_timer = Some(tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    h.abort();
                    *killed_at.lock().unwrap() = Some(now_ms());
                    println!("[coordinator] SPOF: Central controller killed at T+{kill_sec}s");
                }));
            }
        }

        let (k6_summary, wall) = run_k6(c, &entry.url).await?;
        if let Some(t) = spof_timer {
            t.abort();
        }
        if let Some(h) = central {
            h.abort();
        }
        let spof = if c.spof_kill_at_sec > 0 {
            json!({ "killAtSec": c.spof_kill_at_sec, "killedAtMs": *killed_at.lock().unwrap() })
        } else {
            Value::Null
        };

        let service_metrics = collect_metrics(&client, c, &topo.services).await;
        let meta = ResultMeta { cfg: c, seed, experiment_start_time: experiment_start, spof, timestamp_iso: iso8601(now_ms()) };
        let metrics = k6_summary.get("metrics").cloned().unwrap_or_else(|| json!({}));
        let result = build_result(&metrics, wall, service_metrics, &meta);
        let sr = result["summary"]["successRate"].as_f64().unwrap_or(0.0);
        println!(
            "[coordinator] seed={seed} SR={}% P90={}ms",
            js_to_fixed(sr * 100.0, 1),
            js_number(result["summary"]["p90Ms"].as_f64().unwrap_or(0.0))
        );
        let file = result_filename(c, seed, now_ms());
        std::fs::write(&file, to_js_json_pretty(&result)).map_err(|e| format!("{}: {e}", file.display()))?;
        println!("[coordinator] Saved {}", file.display());
        written.push(file);
    }
    println!("\n[coordinator] Cell complete: {} seeds written to {}/", c.seeds.len(), c.results_dir.display());
    Ok(written)
}
