//! US1 unit tests against golden vectors from the JS reference (tasks T018, T024, T025).

use metastable::ews::*;
use metastable::sim::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn golden(name: &str) -> Value {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn hex(x: f64) -> String {
    format!("{:x}", x.to_bits())
}

fn from_hex(v: &Value) -> Option<f64> {
    v.as_str().map(|s| f64::from_bits(u64::from_str_radix(s, 16).unwrap()))
}

fn sha(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Spike R6 (T018/T019): ln/cos/sqrt vs V8 on the simulator/PRNG inputs. std ln/cos differ
/// from V8 in the last bit for ~10% of inputs (ADR-0025), so we use the fdlibm port.
#[test]
fn jsmath_matches_v8() {
    let mut bad = false;
    for g in golden("ln_cos.json").as_array().unwrap() {
        let mut rng = mulberry32(g["seed"].as_f64().unwrap());
        let n = g["n"].as_u64().unwrap() as usize;
        let (mut ln, mut cos, mut sqrt) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
        for _ in 0..n {
            let u = rng();
            let x = 1.0 - u;
            ln.push(hex(metastable::jsmath::log(x)));
            cos.push(hex(metastable::jsmath::cos(2.0 * std::f64::consts::PI * u)));
            sqrt.push(hex((-2.0 * metastable::jsmath::log(x)).sqrt()));
        }
        for (name, v) in [("ln", &ln), ("cos", &cos), ("sqrt", &sqrt)] {
            let sample: Vec<&str> = g["sample"][name].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
            let diff = v.iter().zip(&sample).filter(|(a, b)| a != b).count();
            let ok = sha(&v.join(",")) == g[name].as_str().unwrap();
            eprintln!("seed {} {name}: digest_ok={ok} sample_diffs={diff}/{}", g["seed"], sample.len());
            bad |= !ok;
        }
    }
    assert!(!bad, "math differs from V8 (see stderr)");
}

#[test]
fn mulberry32_matches_js() {
    for g in golden("mulberry32.json").as_array().unwrap() {
        let mut r = mulberry32(g["seed"].as_f64().unwrap());
        for (i, v) in g["values"].as_array().unwrap().iter().enumerate() {
            assert_eq!(hex(r()), v.as_str().unwrap(), "seed {} index {i}", g["seed"]);
        }
    }
}

fn row_bits(r: &Row) -> Vec<String> {
    [r.t, r.goodput, r.offered, r.fresh, r.retry, r.wasted, r.shed, r.timeout, r.queue_len, r.in_service]
        .map(hex)
        .to_vec()
}

#[test]
fn run_trial_is_bit_identical_to_js() {
    let g = golden("sim.json");
    for t in g["trials"].as_array().unwrap() {
        let c = &g["configs"][t["name"].as_str().unwrap()];
        let s = &c["surge"];
        let f = |k: &str| s[k].as_f64().unwrap();
        let lambda = surge_schedule(f("baseline"), f("surge"), f("surgeStartMs"), f("surgeDurMs"));
        let mut cfg = TrialConfig { seed: t["seed"].as_f64().unwrap(), ..Default::default() };
        let o = &c["cfg"];
        let num = |k: &str, d: f64| o.get(k).and_then(Value::as_f64).unwrap_or(d);
        cfg.attempt_timeout_ms = num("attemptTimeoutMs", cfg.attempt_timeout_ms);
        cfg.max_attempts = num("maxAttempts", cfg.max_attempts);
        cfg.retry_backoff_ms = num("retryBackoffMs", cfg.retry_backoff_ms);
        cfg.queue_max = num("queueMax", cfg.queue_max);
        cfg.total_ms = num("totalMs", cfg.total_ms);
        cfg.tick_ms = num("tickMs", cfg.tick_ms);
        cfg.cancel_queued_on_timeout = o.get("cancelQueuedOnTimeout").and_then(Value::as_bool).unwrap_or(false);

        let rows = run_trial(&cfg, lambda).rows;
        let label = format!("{} seed {}", t["name"], t["seed"]);
        if let Some(expected) = t.get("rows") {
            for (i, (r, e)) in rows.iter().zip(expected.as_array().unwrap()).enumerate() {
                let e: Vec<&str> = e.as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
                assert_eq!(row_bits(r), e, "{label}: first differing row {i}");
            }
        }
        assert_eq!(rows.len() as u64, t["nRows"].as_u64().unwrap(), "{label}: row count");
        let digest = sha(&rows.iter().map(|r| row_bits(r).join(",")).collect::<Vec<_>>().join("\n"));
        assert_eq!(digest, t["digest"].as_str().unwrap(), "{label}: digest");
    }
}

#[test]
fn nominal_capacity_matches_js() {
    for n in golden("sim.json")["nominal"].as_array().unwrap() {
        let c = &n["cfg"];
        let cfg = TrialConfig {
            capacity: c.get("capacity").and_then(Value::as_f64).unwrap_or(50.0),
            service_mean_ms: c.get("serviceMeanMs").and_then(Value::as_f64).unwrap_or(100.0),
            ..Default::default()
        };
        assert_eq!(hex(nominal_capacity_rps(&cfg)), n["bits"].as_str().unwrap());
    }
}

#[test]
fn heap_breaks_time_ties_by_insertion_order() {
    // Two arrivals at identical virtual time must fire in insertion order: observable as a
    // deterministic, repeatable trial (ADR-0014).
    let cfg = TrialConfig { total_ms: 2000.0, ..Default::default() };
    let a = run_trial(&cfg, |_| 300.0).rows;
    let b = run_trial(&cfg, |_| 300.0).rows;
    assert_eq!(a, b);
}

#[test]
fn ews_matches_js() {
    for g in golden("ews.json").as_array().unwrap() {
        let w = g["w"].as_u64().unwrap() as usize;
        let input: Vec<Option<f64>> = g["input"].as_array().unwrap().iter().map(from_hex).collect();
        let label = format!("{} w={w}", g["name"]);
        let cmp = |name: &str, got: Vec<Option<f64>>| {
            let exp: Vec<Option<f64>> = g[name].as_array().unwrap().iter().map(from_hex).collect();
            let got: Vec<Option<String>> = got.into_iter().map(|x| x.map(hex)).collect();
            let exp: Vec<Option<String>> = exp.into_iter().map(|x| x.map(hex)).collect();
            assert_eq!(got, exp, "{label} {name}");
        };
        if g.get("rollingMean").is_some() {
            let plain: Vec<f64> = input.iter().map(|x| x.unwrap()).collect();
            cmp("rollingMean", rolling_mean(&plain, w));
            cmp("detrend", detrend(&plain, w));
        }
        cmp("rollingVar", rolling_var(&input, w));
        cmp("rollingAR1", rolling_ar1(&input, w));
        assert_eq!(hex(kendall_tau(&input)), g["kendallTau"].as_str().unwrap(), "{label} kendallTau");
        let ps = [0.0, 0.5, 0.9, 0.95, 0.99, 1.0];
        let got: Vec<Option<String>> = ps.iter().map(|&p| percentile(&input, p).map(hex)).collect();
        let exp: Vec<Option<String>> =
            g["percentile"].as_array().unwrap().iter().map(|v| v.as_str().map(String::from)).collect();
        assert_eq!(got, exp, "{label} percentile");
    }
}
