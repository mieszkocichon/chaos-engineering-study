//! T042: CircuitBreaker, RetryPolicy, ConcurrencyLimiter, MetricsCollector vs the JS reference.

mod common;
use std::cell::RefCell;

use chaos::metrics::MetricsCollector;
use chaos::resilience::*;
use common::*;
use serde_json::{Value, json};

#[test]
fn circuit_breaker_matches_js() {
    let g = golden("resilience.json");
    for (s, case) in g["cb"].as_array().unwrap().iter().enumerate() {
        let start = case["start"].as_u64().unwrap();
        let mut cb = CircuitBreaker::new(num(&case["threshold"]), num(&case["timeout"]), case["name"].as_str().unwrap(), start);
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let t = step["t"].as_u64().unwrap();
            let label = format!("cb case {s} step {i}");
            if let Some(u) = step.get("update") {
                cb.update_params(Some(num(&u["failureThreshold"])), Some(num(&u["openTimeoutMs"])));
            } else {
                let ok = step["ok"].as_bool().unwrap();
                let result = match cb.before_call(t) {
                    Err(e) => e,
                    Ok(()) if ok => {
                        cb.on_success(t);
                        "ok".into()
                    }
                    Ok(()) => {
                        cb.on_failure(t);
                        "boom".into()
                    }
                };
                assert_eq!(result, step["result"].as_str().unwrap(), "{label} result");
            }
            let mut stats = cb.stats();
            let hist = stats.as_object_mut().unwrap().remove("stateHistory").unwrap();
            let hist = hist.as_array().unwrap();
            stats["historyLen"] = json!(hist.len());
            stats["lastTransition"] = hist.last().cloned().unwrap_or(Value::Null);
            assert_json_eq(&stats, &step["stats"], &label);
            assert_eq!(cb.failure_threshold, num(&step["params"]["failureThreshold"]), "{label}");
            assert_eq!(cb.open_timeout_ms, num(&step["params"]["openTimeoutMs"]), "{label}");
        }
    }
}

struct Cell(RefCell<RetryPolicy>);
impl RetryState for Cell {
    fn with_policy<R>(&self, f: impl FnOnce(&mut RetryPolicy) -> R) -> R {
        f(&mut self.0.borrow_mut())
    }
}

#[tokio::test(start_paused = true)]
async fn retry_policy_matches_js() {
    let g = golden("resilience.json");
    for (c, case) in g["retry"].as_array().unwrap().iter().enumerate() {
        let p = &case["params"];
        let mut rp = RetryPolicy::new(num(&p["maxRetries"]), num(&p["baseDelayMs"]), num(&p["backoffMultiplier"]));
        if let Some(m) = p.get("maxDelayMs") {
            rp.max_delay_ms = num(m);
        }
        let script: Vec<String> = case["script"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
        let state = Cell(RefCell::new(rp));
        let attempts = RefCell::new(Vec::new());
        let delays = RefCell::new(Vec::new());
        let last = RefCell::new(tokio::time::Instant::now());
        let k = RefCell::new(0usize);
        let result = execute(&state, |attempt| {
            attempts.borrow_mut().push(attempt);
            let now = tokio::time::Instant::now();
            let since = now - *last.borrow();
            if attempt > 0 {
                delays.borrow_mut().push(since.as_secs_f64() * 1000.0);
            }
            *last.borrow_mut() = now;
            let o = script[(*k.borrow()).min(script.len() - 1)].clone();
            *k.borrow_mut() += 1;
            async move {
                match o.as_str() {
                    "ok" => Ok(format!("ok@{attempt}")),
                    "cb" => Err("CircuitBreakerOpen:x".to_string()),
                    _ => Err(format!("HTTP 502: fail {attempt}")),
                }
            }
        })
        .await;
        let result = match result {
            Ok(v) => v,
            Err(e) => format!("error:{e}"),
        };
        let label = format!("retry case {c} {script:?}");
        assert_eq!(result, case["result"].as_str().unwrap(), "{label}");
        let exp_attempts: Vec<u32> = case["attempts"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
        assert_eq!(*attempts.borrow(), exp_attempts, "{label}");
        // JS records every scheduled delay; attempts after a delay prove the sleep happened
        let exp_delays: Vec<f64> = case["delays"].as_array().unwrap().iter().map(num).collect();
        let pol = state.0.borrow();
        let computed: Vec<f64> = (0..exp_delays.len() as u32).map(|a| pol.delay_ms(a)).collect();
        assert_eq!(computed, exp_delays, "{label} delays");
        for (d, e) in delays.borrow().iter().zip(&exp_delays) {
            assert!((d - e).abs() < 1.0, "{label}: slept {d} ms, expected {e}");
        }
        assert_json_eq(&pol.stats(), &case["stats"], &label);
    }
}

#[test]
fn concurrency_limiter_matches_js() {
    let g = golden("resilience.json");
    let mut lim = ConcurrencyLimiter::new(3.0);
    for (i, step) in g["limiter"].as_array().unwrap().iter().enumerate() {
        match step["op"].as_str().unwrap() {
            "acquire" => {
                let r = lim.acquire().err().unwrap_or_else(|| "ok".into());
                assert_eq!(r, step["result"].as_str().unwrap(), "limiter step {i}");
            }
            "release" => lim.release(),
            _ => lim.update_params(Some(num(&step["max"]))),
        }
        assert_json_eq(&lim.stats(), &step["stats"], &format!("limiter step {i}"));
    }
}

#[test]
fn metrics_collector_matches_js() {
    let g = golden("resilience.json");
    for (c, case) in g["metrics"].as_array().unwrap().iter().enumerate() {
        let mut m = MetricsCollector::default();
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let t = step["t"].as_u64().unwrap();
            match step["op"].as_str().unwrap() {
                "start" => m.start_request(),
                "record" => m.record_request(t, num(&step["latencyMs"]), step["success"].as_bool().unwrap()),
                "reset" => m.reset(),
                _ => {}
            }
            let label = format!("metrics case {c} step {i}");
            assert_json_eq(&serde_json::to_value(m.snapshot(t)).unwrap(), &step["snapshot"], &label);
            assert_json_eq(&serde_json::to_value(m.health_summary(t)).unwrap(), &step["health"], &label);
        }
    }
}

/// Quirk Q2 (research R10): the limiter rejects *before* retry/CB, so a shed request is
/// neither retried nor counted as a breaker failure.
#[test]
fn limiter_rejection_is_not_retried_nor_a_cb_failure() {
    let mut lim = ConcurrencyLimiter::new(0.0);
    let cb = CircuitBreaker::new(1.0, 1000.0, "x", 0);
    let err = lim.acquire().unwrap_err();
    assert_eq!(err, "ConcurrencyLimitExceeded: 0/0");
    assert_eq!(cb.failure_count, 0);
}
