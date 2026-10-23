//! T059: multi-process integration on localhost – gossip, /control, soft-reset determinism,
//! startup validation. Topology: tests/fixtures/topologies/line-3.json.

mod common;
use std::process::Command;
use std::time::{Duration, Instant};

use chaos::prng::Prng;
use common::svc::*;
use serde_json::{Value, json};

async fn get(url: &str) -> Value {
    reqwest::get(url).await.unwrap().json().await.unwrap()
}

async fn post(url: &str, body: Value) -> Value {
    reqwest::Client::new().post(url).json(&body).send().await.unwrap().json().await.unwrap()
}

fn line3(strategy: &str) -> (u16, Vec<Svc>) {
    let base = free_base_port(3);
    let cfg = fixtures_dir();
    let procs = vec![
        spawn("svc-01", "line-3", &cfg, base, base + 1, "none", &[]),
        spawn("svc-02", "line-3", &cfg, base, base + 2, "none", &[]),
        spawn("gateway", "line-3", &cfg, base, base, strategy, &[]),
    ];
    (base, procs)
}

#[tokio::test(flavor = "multi_thread")]
async fn leaves_gossip_to_mas_gateway() {
    let (base, _p) = line3("mas-agent");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let m = get(&format!("http://127.0.0.1:{base}/metrics")).await;
        let st = &m["strategyStats"];
        assert_eq!(st["strategy"], "mas-agent");
        if st["neighborCount"].as_u64().unwrap() >= 2 {
            break; // LeafGossipPusher of both leaves reached the gateway
        }
        assert!(Instant::now() < deadline, "no gossip after 10 s: {st}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    // decision ticks every 2 s
    tokio::time::sleep(Duration::from_millis(2300)).await;
    let m = get(&format!("http://127.0.0.1:{base}/metrics")).await;
    assert!(m["strategyStats"]["decisionCount"].as_u64().unwrap() >= 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn control_updates_central_params() {
    let (base, _p) = line3("central");
    let gw = format!("http://127.0.0.1:{base}");
    // create pipelines first so the update must reach them
    let d = get(&format!("{gw}/data")).await;
    assert_eq!(d["downstreamResults"].as_array().unwrap().len(), 2);
    post(&format!("{gw}/control"), json!({ "retryCount": 1, "cbThreshold": 9, "concurrencyLimit": 7 })).await;
    let st = get(&format!("{gw}/metrics")).await["strategyStats"].clone();
    assert_eq!(st["currentParams"]["retryCount"], 1.0);
    assert_eq!(st["svc-01"]["circuitBreaker"]["state"], "closed");
    assert_eq!(st["svc-01"]["retry"]["currentParams"]["maxRetries"], 1.0);
    assert_eq!(st["svc-01"]["concurrency"]["max"], 7.0);
}

/// Soft reset re-seeds the delay PRNG: latencies follow `normal(50,10)` drawn from
/// `"<seed>_svc-01_delay"`, identically after every reset with that seed.
#[tokio::test(flavor = "multi_thread")]
async fn reset_reseeds_processing_delays() {
    let (base, _p) = line3("none");
    let leaf = format!("http://127.0.0.1:{}", base + 1);
    let mut expected = Prng::new("7000_svc-01_delay");
    let expected: Vec<f64> = (0..8).map(|_| metastable::js::js_round(expected.normal_random(50.0, 10.0)).max(0.0)).collect();
    for round in 0..2 {
        post(&format!("{leaf}/experiment/reset"), json!({ "seed": 7000 })).await;
        for (i, e) in expected.iter().enumerate() {
            let lat = get(&format!("{leaf}/data")).await["latencyMs"].as_f64().unwrap();
            assert!(lat >= *e && lat <= e + 25.0, "round {round} call {i}: latency {lat} vs planned delay {e}");
        }
    }
}

#[test]
fn unknown_service_id_lists_available_ids() {
    let out = Command::new(env!("CARGO_BIN_EXE_chaos-svc"))
        .env("SERVICE_ID", "svc-99")
        .env("TOPOLOGY", "line-3")
        .env("FAULT_PROFILE", "no-wan")
        .env("CONFIG_DIR", fixtures_dir())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("SERVICE_ID=\"svc-99\" not found in topology \"line-3\""), "{err}");
    assert!(err.contains("Available: gateway, svc-01, svc-02"), "{err}");
}

#[test]
fn missing_service_id_and_unknown_strategy_fail() {
    let out = Command::new(env!("CARGO_BIN_EXE_chaos-svc")).env_remove("SERVICE_ID").output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("SERVICE_ID env var is required"));

    let out = Command::new(env!("CARGO_BIN_EXE_chaos-svc"))
        .env("SERVICE_ID", "gateway")
        .env("TOPOLOGY", "line-3")
        .env("FAULT_PROFILE", "no-wan")
        .env("STRATEGY", "magic")
        .env("CONFIG_DIR", fixtures_dir())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Unknown strategy: magic"));
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_static_params_fall_back_to_defaults() {
    let base = free_base_port(3);
    let cfg = fixtures_dir();
    let _leaf = spawn("svc-01", "line-3", &cfg, base, base + 1, "none", &[]);
    let gw = spawn("gateway", "line-3", &cfg, base, base, "static", &[("STATIC_PARAMS", "{not json")]);
    get(&format!("{}/data", gw.url)).await;
    let st = get(&format!("{}/metrics", gw.url)).await["strategyStats"].clone();
    assert_eq!(st["svc-01"]["retry"]["currentParams"]["maxRetries"], 3.0);
    assert_eq!(st["svc-01"]["concurrency"]["max"], 50.0);
}
