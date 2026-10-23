//! T058: HTTP contract of the experiment service (contracts/service-http.md).
//! Runs against a spawned `chaos-svc`; set `CONTRACT_TARGET_LEAF` / `CONTRACT_TARGET_GATEWAY`
//! to base URLs to run the same table against the JS reference instead.

mod common;
use common::svc::*;
use serde_json::{Value, json};

struct Target {
    leaf: String,
    gateway: String,
    _procs: Vec<Svc>,
}

fn targets(gateway_strategy: &str) -> Target {
    if let (Ok(leaf), Ok(gateway)) = (std::env::var("CONTRACT_TARGET_LEAF"), std::env::var("CONTRACT_TARGET_GATEWAY")) {
        return Target { leaf, gateway, _procs: vec![] };
    }
    // ring-5: gateway (port base), svc-01..05 (base+1..); only gateway + svc-01 run, so most
    // downstream calls of the gateway fail – exactly what the error-path cases need.
    let base = free_base_port(6);
    let cfg = config_dir();
    let leaf = spawn("svc-01", "ring-5", &cfg, base, base + 1, "none", &[]);
    let gw = spawn("gateway", "ring-5", &cfg, base, base, gateway_strategy, &[]);
    Target { leaf: leaf.url.clone(), gateway: gw.url.clone(), _procs: vec![leaf, gw] }
}

async fn call(method: &str, url: &str, body: Option<Value>) -> (u16, String, Value) {
    let c = reqwest::Client::new();
    let req = if method == "GET" { c.get(url) } else { c.post(url) };
    let req = match body {
        Some(b) => req.json(&b),
        None => req,
    };
    let res = req.send().await.unwrap();
    let status = res.status().as_u16();
    let ct = res.headers().get("content-type").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    (status, ct, res.json().await.unwrap())
}

fn keys(v: &Value) -> Vec<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn endpoints_follow_the_contract() {
    let t = targets("none");
    let (leaf, gw) = (&t.leaf, &t.gateway);

    let (s, ct, v) = call("GET", &format!("{leaf}/health"), None).await;
    assert_eq!(s, 200);
    assert!(ct.starts_with("application/json"), "{ct}");
    assert_eq!(keys(&v), ["status", "service", "uptime"]);
    assert_eq!(v["status"], "ok");
    assert_eq!(v["service"], "svc-01");
    assert!(v["uptime"].is_number());

    // leaf: no downstreams → downstreamResults null
    let (s, _, v) = call("GET", &format!("{leaf}/data"), None).await;
    assert_eq!(s, 200);
    assert_eq!(keys(&v), ["service", "latencyMs", "downstreamResults", "timestamp"]);
    assert!(v["downstreamResults"].is_null());
    assert!(v["latencyMs"].as_u64().is_some());

    let (s, _, v) = call("GET", &format!("{leaf}/metrics"), None).await;
    assert_eq!(s, 200);
    assert_eq!(keys(&v), ["service", "snapshot", "resources", "healthSummary", "strategyStats", "timestamp"]);
    assert_eq!(
        keys(&v["snapshot"]),
        ["errorRate", "avgLatency", "p50", "p90", "p99", "requestCount", "queueDepth", "windowMs"]
    );
    assert_eq!(v["snapshot"]["requestCount"], 1);
    assert_eq!(keys(&v["resources"]), ["memoryMb", "cpuUser", "cpuSystem", "uptime"]);
    assert_eq!(keys(&v["healthSummary"]), ["errorRate", "avgLatency", "load"]);
    assert!(v["strategyStats"].is_null(), "leaf has no strategy");

    for path in ["/gossip", "/control"] {
        let (s, _, v) = call("POST", &format!("{leaf}{path}"), Some(json!({"fromId": "x"}))).await;
        assert_eq!((s, v), (200, json!({"ok": true})), "{path}");
    }

    let (s, _, v) = call("POST", &format!("{leaf}/start"), Some(json!({"experimentStartTime": 123}))).await;
    assert_eq!((s, v), (200, json!({"ok": true, "experimentStartTime": 123, "service": "svc-01"})));
    let (_, _, v) = call("POST", &format!("{leaf}/start"), None).await;
    assert!(v["experimentStartTime"].as_u64().unwrap() > 1_600_000_000_000);

    let (s, _, v) = call("POST", &format!("{leaf}/experiment/reset"), Some(json!({"seed": 7000}))).await;
    assert_eq!((s, v), (200, json!({"ok": true, "service": "svc-01", "seed": 7000})));

    // route precedence: /faults/recover beats /faults/:profile
    let (s, _, v) = call("POST", &format!("{leaf}/faults/recover"), None).await;
    assert_eq!((s, v), (200, json!({"status": "recovered"})));
    let (s, _, v) = call("POST", &format!("{leaf}/faults/recover/network-partition"), None).await;
    assert_eq!((s, v), (200, json!({"status": "recovered", "type": "network-partition"})));
    let (s, _, v) = call("POST", &format!("{leaf}/faults/no-such-profile"), Some(json!({}))).await;
    assert_eq!((s, v), (200, json!({"status": "injected", "profile": "no-such-profile"})));

    // gateway with `none`: dead downstreams make Promise.all reject → 502
    let (s, _, v) = call("GET", &format!("{gw}/data"), None).await;
    assert_eq!(s, 502);
    assert_eq!(keys(&v), ["error", "service"]);
    assert_eq!(v["service"], "gateway");
}

#[tokio::test(flavor = "multi_thread")]
async fn reset_clears_metrics() {
    let t = targets("none");
    call("GET", &format!("{}/data", t.leaf), None).await;
    call("POST", &format!("{}/experiment/reset", t.leaf), Some(json!({"seed": 1}))).await;
    let (_, _, v) = call("GET", &format!("{}/metrics", t.leaf), None).await;
    assert_eq!(v["snapshot"]["requestCount"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn pipelined_strategy_degrades_instead_of_failing() {
    let t = targets("static");
    let (s, _, v) = call("GET", &format!("{}/data", t.gateway), None).await;
    assert_eq!(s, 200);
    let results = v["downstreamResults"].as_array().unwrap();
    assert_eq!(results.len(), 5);
    let ok = results.iter().filter(|r| r["service"] == "svc-01" && r.get("degraded").is_none()).count();
    assert_eq!(ok, 1, "svc-01 is up: {results:?}");
    for r in results.iter().filter(|r| r.get("degraded").is_some()) {
        assert_eq!(keys(r), ["error", "service", "degraded"]);
        assert_eq!(r["degraded"], true);
    }
    let (_, _, m) = call("GET", &format!("{}/metrics", t.gateway), None).await;
    let st = &m["strategyStats"];
    assert_eq!(st["strategy"], "static");
    assert_eq!(keys(&st["svc-02"]), ["circuitBreaker", "retry", "concurrency"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_only_strategy_degrades_and_reports_stats() {
    let t = targets("retry-only");
    let (s, _, v) = call("GET", &format!("{}/data", t.gateway), None).await;
    assert_eq!(s, 200);
    let results = v["downstreamResults"].as_array().unwrap();
    assert_eq!(results.len(), 5);
    let ok = results.iter().filter(|r| r["service"] == "svc-01" && r.get("degraded").is_none()).count();
    assert_eq!(ok, 1, "svc-01 is up: {results:?}");
    for r in results.iter().filter(|r| r.get("degraded").is_some()) {
        assert_eq!(keys(r), ["error", "service", "degraded"]);
        assert_eq!(r["degraded"], true);
    }
    let (_, _, m) = call("GET", &format!("{}/metrics", t.gateway), None).await;
    let st = &m["strategyStats"];
    assert_eq!(st["strategy"], "retry-only");
    assert!(st["retryPolicies"]["svc-01"]["totalAttempts"].as_u64().unwrap() >= 1);
}
