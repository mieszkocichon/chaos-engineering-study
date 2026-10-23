//! T065 + T066: result file building/naming and the fault schedule, against historical
//! result files of the JS coordinator.

mod common;
use std::path::PathBuf;

use chaos::coordinator::*;
use chaos::topology::load_topology;
use metastable::js::to_js_json_pretty;
use serde_json::{Value, json};

fn repo() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn cfg_from(config: &Value) -> Config {
    let s = |k: &str| config[k].as_str().unwrap_or_default().to_string();
    Config {
        topology: s("topology"),
        fault_base: s("faultBase"),
        fault_profile: s("faultProfile"),
        fault_partition: s("faultPartition"),
        strategy: s("strategy"),
        seeds: vec![config["seed"].as_i64().unwrap()],
        rps: config["rps"].as_i64().unwrap(),
        duration_sec: config["durationSec"].as_i64().unwrap(),
        warmup_sec: config["warmupSec"].as_i64().unwrap(),
        base_port: 4000,
        results_dir: PathBuf::from("/results"),
        health_timeout_ms: 300_000,
        inter_seed_settle_ms: 2000,
        spof_kill_at_sec: 0,
        k6_bin: "k6".into(),
        k6_script: String::new(),
        config_dir: repo().join("when-gossip-goes-silent/config"),
        host_override: None,
    }
}

/// A spread of historical files: several strategies, both bases, a partition run, SPOF.
fn samples() -> Vec<PathBuf> {
    let mut out = vec![];
    for (dir, needles) in [
        ("results", &["_none_no-wan_baseline-flaky_1000_", "_mas-agent_wan-50ms_burst-failure_", "_backpressure_no-wan_baseline-flaky+network-partition_", "_central_no-wan_slow-degradation_", "_static_wan-50ms_cascade-crash_"][..]),
        ("results-spof", &["_central_"][..]),
        ("results-swim", &["_mas-agent-swim_"][..]),
    ] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(repo().join("when-gossip-goes-silent").join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        files.sort();
        for n in needles {
            if let Some(f) = files.iter().find(|f| f.to_string_lossy().contains(n)) {
                out.push(f.clone());
            }
        }
    }
    assert!(out.len() >= 6, "{out:?}");
    out
}

#[test]
fn build_result_reproduces_historical_files() {
    for f in samples() {
        let text = std::fs::read_to_string(&f).unwrap();
        let hist: Value = serde_json::from_str(&text).unwrap();
        let cfg = cfg_from(&hist["config"]);
        let meta = ResultMeta {
            cfg: &cfg,
            seed: hist["config"]["seed"].as_i64().unwrap(),
            experiment_start_time: hist["experimentStartTime"].as_u64().unwrap(),
            spof: hist["config"]["spof"].clone(),
            timestamp_iso: hist["timestamp"].as_str().unwrap().into(),
        };
        let wall = hist["summary"]["wallClockMs"].as_u64().unwrap();
        let mut got = build_result(&hist["k6Metrics"], wall, hist["serviceMetrics"].clone(), &meta);
        // files written before the JS coordinator gained the SPOF hook have no `config.spof`
        if hist["config"].get("spof").is_none() {
            got["config"].as_object_mut().unwrap().remove("spof");
        }
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(got["summary"], hist["summary"], "{name}: summary");
        assert_eq!(got["config"], hist["config"], "{name}: config");
        // whole file, byte for byte: JS-compatible number formatting and key order
        assert_eq!(to_js_json_pretty(&got), text, "{name}: file text");
    }
}

#[test]
fn result_filename_matches_js_pattern() {
    let mut c = cfg_from(&json!({"topology": "ring-50", "strategy": "mas-agent", "faultBase": "no-wan",
        "faultProfile": "baseline-flaky", "faultPartition": null, "seed": 1000, "rps": 20, "durationSec": 60, "warmupSec": 15}));
    assert_eq!(result_filename(&c, 1000, 42), PathBuf::from("/results/ring-50_mas-agent_no-wan_baseline-flaky_1000_42.json"));
    c.fault_partition = "network-partition".into();
    assert_eq!(
        result_filename(&c, 7000, 1),
        PathBuf::from("/results/ring-50_mas-agent_no-wan_baseline-flaky+network-partition_7000_1.json")
    );
}

#[test]
fn iso8601_like_to_iso_string() {
    assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(iso8601(1_776_540_455_098), "2026-04-18T19:27:35.098Z");
    assert_eq!(iso8601(951_782_400_000), "2000-02-29T00:00:00.000Z");
}

fn services() -> Vec<chaos::topology::Service> {
    load_topology(&repo().join("when-gossip-goes-silent/config"), "ring-5", 4000, &|id| id.to_string()).unwrap().services
}

#[test]
fn schedule_inject_and_type_scoped_recover() {
    let profile = json!({ "faults": [
        { "targetService": "svc-01", "type": "baseline-flaky", "startTimeSec": 5, "endTimeSec": 999 },
        { "targetService": "svc-02", "type": "burst-failure", "startTimeSec": 15, "endTimeSec": 40 },
        { "targetService": "svc-03", "type": "cascade-crash", "startTimeSec": 20 },
        { "targetService": "svc-04", "type": "cascade-crash", "startTimeSec": 20, "endTimeSec": 0 },
        { "targetService": "gateway", "type": "wan-baseline", "startTimeSec": 1, "endTimeSec": 999, "options": { "delayMs": 50, "jitterMs": 10 } },
        { "targetService": "svc-99", "type": "burst-failure", "startTimeSec": 1 },
        { "targetService": "svc-05", "type": "slow-degradation" },
    ]});
    let opts = |f: &Value| f.get("options").cloned().unwrap_or_else(|| json!({}));
    let got = plan_faults(&profile, &services(), &|id| id.to_string(), &opts);
    let a = |ms: u64, svc: &str, port: u16, path: &str, body: Option<Value>| FaultAction {
        at_ms: ms,
        service: svc.into(),
        url: format!("http://{svc}:{port}/faults/{path}"),
        body,
    };
    assert_eq!(
        got,
        vec![
            a(5000, "svc-01", 4001, "baseline-flaky", Some(json!({}))),
            a(15000, "svc-02", 4002, "burst-failure", Some(json!({}))),
            a(40000, "svc-02", 4002, "recover/burst-failure", None),
            a(20000, "svc-03", 4003, "cascade-crash", Some(json!({}))),
            a(35000, "svc-03", 4003, "recover/cascade-crash", None),
            a(20000, "svc-04", 4004, "cascade-crash", Some(json!({}))),
            a(35000, "svc-04", 4004, "recover/cascade-crash", None), // `!0` → default 35 s
            a(1000, "gateway", 4000, "wan-baseline", Some(json!({ "delayMs": 50, "jitterMs": 10 }))),
            a(0, "svc-05", 4005, "slow-degradation", Some(json!({}))), // no endTimeSec → persistent
        ]
    );
}

#[test]
fn shipped_profiles_plan_without_surprises() {
    let dir = repo().join("when-gossip-goes-silent/config");
    let p = chaos::topology::load_fault_profile(&dir, "network-partition").unwrap();
    let got = plan_faults(&p, &services(), &|id| id.to_string(), &|_| json!({ "peerIps": ["10.0.0.3"] }));
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].at_ms, 5000);
    assert_eq!(got[1].url, "http://gateway:4000/faults/recover/network-partition");
    assert_eq!(got[1].at_ms, 75000);
}
