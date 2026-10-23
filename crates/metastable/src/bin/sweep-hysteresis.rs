//! M1 hysteresis sweep – port of `metastable-failures/src/sweep-hysteresis.js` (byte-identical output).
//!   sweep-hysteresis --lmin 100 --lmax 900 --steps 16 --dwell 4000

use metastable::args::Args;
use metastable::js::{js_fixed_num, js_round, to_js_json_pretty};
use metastable::sim::{TrialConfig, nominal_capacity_rps, run_trial};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
struct Point {
    lambda: f64,
    goodput: f64,
}

fn main() {
    let o = Args::from_env();
    let mut model = TrialConfig {
        capacity: o.num("capacity", 50.0),
        service_mean_ms: o.num("service-ms", 100.0),
        queue_max: o.num("qmax", 500.0),
        attempt_timeout_ms: o.num("timeout", 300.0),
        max_attempts: o.num("attempts", 30.0),
        retry_backoff_ms: o.num("backoff", 30.0),
        tick_ms: o.num("tick", 200.0),
        seed: o.num("seed", 1.0),
        ..Default::default()
    };
    let lmin = o.num("lmin", 100.0);
    let lmax = o.num("lmax", 900.0);
    let steps = o.num("steps", 16.0);
    let dwell_ms = o.num("dwell", 4000.0);

    // staircase: up lmin->lmax then down lmax->lmin, each level held for dwellMs
    let level = |i: f64| lmin + ((lmax - lmin) * i) / (steps - 1.0);
    let mut levels_up = Vec::new();
    let mut i = 0.0;
    while i < steps {
        levels_up.push(level(i));
        i += 1.0;
    }
    let mut levels_down = Vec::new();
    let mut i = steps - 1.0;
    while i >= 0.0 {
        levels_down.push(level(i));
        i -= 1.0;
    }
    let schedule: Vec<f64> = levels_up.iter().chain(&levels_down).copied().collect();
    model.total_ms = schedule.len() as f64 * dwell_ms;
    let sched = schedule.clone();
    let lambda = move |t: f64| {
        let idx = ((t / dwell_ms).floor()).min(sched.len() as f64 - 1.0);
        sched[idx as usize]
    };

    let rows = run_trial(&model, lambda).rows;

    // mean goodput over the last 40% of each dwell (steady part)
    let level_goodput = |level_index: f64| {
        let lo = level_index * dwell_ms + 0.6 * dwell_ms;
        let hi = (level_index + 1.0) * dwell_ms;
        let w: Vec<f64> = rows.iter().filter(|r| r.t > lo && r.t <= hi).map(|r| r.goodput).collect();
        if w.is_empty() { 0.0 } else { w.iter().fold(0.0, |s, g| s + g) / w.len() as f64 }
    };

    let nom_cap = nominal_capacity_rps(&model);
    let is_collapsed = |p: &Point| p.goodput < 0.5 * p.lambda.min(nom_cap);

    let up: Vec<Point> = levels_up
        .iter()
        .enumerate()
        .map(|(i, &lam)| Point { lambda: js_round(lam), goodput: js_round(level_goodput(i as f64)) })
        .collect();
    let down: Vec<Point> = levels_down
        .iter()
        .enumerate()
        .map(|(i, &lam)| Point { lambda: js_round(lam), goodput: js_round(level_goodput(steps + i as f64)) })
        .collect();

    let lambda_up = up.iter().find(|p| is_collapsed(p)).map(|p| p.lambda);
    let lambda_down = down.iter().find(|p| !is_collapsed(p)).map(|p| p.lambda);
    let width = match (lambda_up, lambda_down) {
        (Some(u), Some(d)) => Some(u - d),
        _ => None,
    };
    let opt = |v: Option<f64>| v.map_or(Value::Null, |v| json!(v));
    let leg = |ps: &[Point]| -> Value {
        ps.iter().map(|p| json!({ "lambda": p.lambda, "goodput": p.goodput })).collect()
    };
    let bistable = width.is_some_and(|w| w > 0.0);

    if o.is_set("json") {
        let result = json!({
            "nominal_capacity_rps": js_fixed_num(nom_cap, 1),
            "lambda_up": opt(lambda_up), "lambda_down": opt(lambda_down),
            "hysteresis_width": opt(width), "bistable": bistable,
            "up_leg": leg(&up), "down_leg": leg(&down),
        });
        println!("{}", to_js_json_pretty(&result));
        return;
    }

    println!(
        "{}",
        to_js_json_pretty(&json!({
            "nominal_capacity_rps": js_fixed_num(nom_cap, 1),
            "lambda_up": opt(lambda_up), "lambda_down": opt(lambda_down),
            "hysteresis_width": opt(width), "bistable": bistable,
        }))
    );
    // ASCII hysteresis loop: goodput vs lambda, U=up-leg D=down-leg
    let max_g = up.iter().chain(&down).fold(1.0f64, |m, p| if p.goodput > m { p.goodput } else { m });
    let h = 12;
    println!("\ngoodput vs offered-rate (U=up-leg, D=down-leg):");
    for row in (0..=h).rev() {
        let thr = (max_g * row as f64) / h as f64;
        let line: String = (0..up.len())
            .map(|i| {
                let u = up[i].goodput >= thr;
                let d = down[down.len() - 1 - i].goodput >= thr; // align by lambda
                match (u, d) {
                    (true, true) => '#',
                    (true, false) => 'U',
                    (false, true) => 'D',
                    _ => ' ',
                }
            })
            .collect();
        println!("{:>5} |{line}", metastable::js::js_number(js_round(thr)));
    }
    println!("      +{}", "-".repeat(up.len()));
    let xs: String = up.iter().map(|p| ((p.lambda / 100.0) as i32).to_string()).collect();
    println!("       {xs}  (x100 rps)");
}
