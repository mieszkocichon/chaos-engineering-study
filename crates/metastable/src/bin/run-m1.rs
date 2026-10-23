//! M1 Gate A probe – port of `metastable-failures/src/run-m1.js` (byte-identical output).
//!   run-m1 --baseline 300 --surge 1500 --surge-dur 10000 [--json]

use metastable::args::Args;
use metastable::js::{js_fixed_num, to_js_json_pretty};
use metastable::sim::{Row, TrialConfig, nominal_capacity_rps, run_trial, surge_schedule};
use serde_json::{Value, json};

fn main() {
    let o = Args::from_env();
    let base = TrialConfig {
        capacity: o.num("capacity", 50.0),
        service_mean_ms: o.num("service-ms", 100.0),
        queue_max: o.num("qmax", 200.0),
        attempt_timeout_ms: o.num("timeout", 500.0),
        max_attempts: o.num("attempts", 5.0),
        retry_backoff_ms: o.num("backoff", 50.0),
        cancel_queued_on_timeout: o.is_true("cancel-queued"),
        total_ms: o.num("total", 60000.0),
        tick_ms: o.num("tick", 200.0),
        seed: o.num("seed", 1.0),
    };
    let baseline = o.num("baseline", 300.0);
    let surge = o.num("surge", 1500.0);
    let surge_start_ms = o.num("surge-start", 15000.0);
    let surge_dur_ms = o.num("surge-dur", 10000.0);
    let settle_ms = o.num("settle", 10000.0);

    let rows = run_trial(&base, surge_schedule(baseline, surge, surge_start_ms, surge_dur_ms)).rows;

    let mean = |xs: &[f64]| if xs.is_empty() { 0.0 } else { xs.iter().fold(0.0, |s, x| s + x) / xs.len() as f64 };
    let gp = |lo: f64, hi: f64| {
        let w: Vec<f64> = rows.iter().filter(|r| r.t > lo && r.t <= hi).map(|r| r.goodput).collect();
        mean(&w)
    };

    let surge_end = surge_start_ms + surge_dur_ms;
    let baseline_goodput = gp(5000.0, surge_start_ms);
    let surge_goodput = gp(surge_start_ms, surge_end);
    let post_goodput = gp(surge_end + settle_ms, base.total_ms);
    let recovered = post_goodput >= 0.9 * baseline_goodput;
    let collapse_ratio = if baseline_goodput > 0.0 { post_goodput / baseline_goodput } else { 1.0 };

    let nom_cap = nominal_capacity_rps(&base);
    let summary = json!({
        "nominal_capacity_rps": js_fixed_num(nom_cap, 1),
        "baseline_rps": baseline,
        "surge_rps": surge,
        "baseline_goodput": js_fixed_num(baseline_goodput, 1),
        "surge_goodput": js_fixed_num(surge_goodput, 1),
        "post_goodput": js_fixed_num(post_goodput, 1),
        "post_over_baseline": js_fixed_num(collapse_ratio, 3),
        "recovered": recovered,
        "metastable_stuck": !recovered,
    });

    if o.is_set("json") {
        let rows: Vec<Value> = rows.iter().map(Row::to_json).collect();
        println!("{}", to_js_json_pretty(&json!({ "summary": summary, "rows": rows })));
    } else {
        println!("{}", to_js_json_pretty(&summary));
        // compact goodput sparkline over time
        let max_g = rows.iter().fold(1.0f64, |m, r| js_max(m, r.goodput));
        let ramp: Vec<char> = "▁▂▃▄▅▆▇█".chars().collect();
        let step = (rows.len() as f64 / 80.0).ceil() as usize;
        let spark: String = rows
            .iter()
            .enumerate()
            .filter(|(i, _)| step != 0 && i % step == 0)
            .map(|(_, r)| ramp[((r.goodput / max_g) * 7.0).floor().min(7.0) as usize])
            .collect();
        println!("goodput |{spark}|  (surge window in the middle)");
    }
}

/// `Math.max` for two finite numbers.
fn js_max(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}
