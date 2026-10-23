//! T084: pure logic of the demos – breaker of gateway-retry-cb, client statistics,
//! saga consistency scorecard, compare verdicts.

use demos::clients::*;
use demos::services::DemoBreaker;
use serde_json::json;

#[test]
fn demo_breaker_opens_half_opens_and_closes() {
    let mut b = DemoBreaker::new("B", 3, 5000);
    for t in [0, 10, 20] {
        b.before(t).unwrap();
        b.failure(t);
    }
    assert_eq!(b.state, "open");
    assert_eq!(b.next_attempt, 20 + 5000);
    assert_eq!(b.before(100).unwrap_err(), "CircuitBreakerOpen: service B unavailable");
    b.before(5020).unwrap(); // timeout elapsed → half-open, one trial call
    assert_eq!(b.state, "half-open");
    b.failure(5020); // a failed trial re-opens immediately
    assert_eq!((b.state, b.next_attempt), ("open", 10_020));
    b.before(10_020).unwrap();
    b.success();
    assert_eq!((b.state, b.failure_cnt), ("closed", 0));
}

#[test]
fn breaker_threshold_counts_consecutive_failures_only() {
    let mut b = DemoBreaker::new("A", 5, 10_000);
    for _ in 0..4 {
        b.failure(0);
    }
    b.success();
    for _ in 0..4 {
        b.failure(0);
    }
    assert_eq!(b.state, "closed");
}

#[test]
fn client_percentile_and_histogram_match_js() {
    // durations[floor(n*p)] on the sorted list
    let d = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
    assert_eq!(pct(&d, 0.5), 60);
    assert_eq!(pct(&d, 0.9), 100);
    assert_eq!(pct(&d, 0.99), 100);
    assert_eq!(pct(&[], 0.5), 0);
    assert_eq!(histogram(&[0, 199, 200, 999, 1000, 4999, 5000, 9999, 10_000, 60_000]), [2, 2, 2, 2, 2]);
}

#[test]
fn saga_scorecard() {
    let orders = json!({ "total": 5, "orders": [
        { "status": "created" }, { "status": "created" }, { "status": "created" },
        { "status": "cancelled" }, { "status": "cancelled" } ] });
    assert_eq!(consistency(&orders, 1), Consistency { created: 3, cancelled: 2, orphaned: 2, duplicates: 0 });
    assert_eq!(consistency(&orders, 5), Consistency { created: 3, cancelled: 2, orphaned: 0, duplicates: 2 });
    assert_eq!(consistency(&json!({}), 0), Consistency { created: 0, cancelled: 0, orphaned: 0, duplicates: 0 });
}

#[test]
fn compare_verdicts() {
    let cb = |sr: f64, p90: f64, rps: f64| json!({ "scenario": "circuit-breaker", "summary": { "successRate": sr, "p90": p90, "rps": rps } });
    assert_eq!(verdict(&cb(0.8, 600.0, 280.0), &cb(1.0, 535.0, 340.0)), "VERDICT: System IMPROVED after applying pattern");
    assert_eq!(verdict(&cb(1.0, 500.0, 100.0), &cb(1.0, 700.0, 100.0)), "VERDICT: System DEGRADED");
    assert_eq!(verdict(&cb(1.0, 500.0, 100.0), &cb(1.0, 550.0, 100.0)), "VERDICT: No significant change");
    let saga = |o: u64, d: u64| json!({ "scenario": "saga", "summary": { "orphaned": o, "duplicates": d } });
    assert_eq!(verdict(&saga(8, 0), &saga(0, 0)), "VERDICT: Data consistency RESTORED! Saga pattern working.");
    assert_eq!(verdict(&saga(0, 0), &saga(0, 0)), "VERDICT: Before was already consistent (re-run with more orders?)");
    assert_eq!(verdict(&saga(8, 0), &saga(2, 0)), "VERDICT: Improved but still inconsistent");
    assert_eq!(verdict(&saga(2, 0), &saga(3, 0)), "VERDICT: Still inconsistent — saga not working yet");
    assert_eq!(pct_change(0.0, 0.0), 0.0);
    assert!(pct_change(0.0, 1.0).is_infinite());
    assert_eq!(pct_change(200.0, 100.0), -50.0);
}

#[test]
fn historical_demo_results_still_compare() {
    // the JS-era reports in demos/results are read by the Rust `compare`
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../demos/results/");
    let load = |f: &str| serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(format!("{dir}{f}")).unwrap()).unwrap();
    assert_eq!(verdict(&load("before-cb.json"), &load("after-cb.json")), "VERDICT: System IMPROVED after applying pattern");
    assert_eq!(verdict(&load("before-saga.json"), &load("after-saga.json")), "VERDICT: Data consistency RESTORED! Saga pattern working.");
}
