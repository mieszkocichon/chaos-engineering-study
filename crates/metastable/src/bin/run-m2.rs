//! M2 Gate B – port of `metastable-failures/src/run-m2.js` (byte-identical output).
//!   run-m2 --signal queueLen --window 25

use metastable::args::Args;
use metastable::ews::{detrend, kendall_tau, percentile, rolling_ar1, rolling_var};
use metastable::js::{js_fixed_num, to_js_json_pretty};
use metastable::sim::{Row, TrialConfig, nominal_capacity_rps, run_trial};
use serde_json::{Value, json};

struct Analysis {
    var_s: Vec<Option<f64>>,
    tau_var: f64,
    tau_ar1: f64,
    lo: usize,
}

fn main() {
    let o = Args::from_env();
    let mut model = TrialConfig {
        capacity: o.num("capacity", 50.0),
        service_mean_ms: o.num("service-ms", 100.0),
        queue_max: o.num("qmax", 500.0),
        attempt_timeout_ms: o.num("timeout", 300.0),
        max_attempts: o.num("attempts", 3.0),
        retry_backoff_ms: o.num("backoff", 30.0),
        tick_ms: o.num("tick", 200.0),
        seed: o.num("seed", 1.0),
        ..Default::default()
    };
    let w = o.num("window", 25.0);
    let signal_name = o.str_or("signal", "queueLen").to_string();
    let nom_cap = nominal_capacity_rps(&model);

    let l0 = o.num("l0", 200.0);
    let l1 = o.num("l1", 560.0);
    let ramp_ms = o.num("ramp", 60000.0);
    let total_ms = ramp_ms + o.num("hold", 12000.0);
    let ramp_lambda = move |t: f64| if t >= ramp_ms { l1 } else { l0 + ((l1 - l0) * t) / ramp_ms };
    let stable_lam = o.num("stable", 260.0);

    model.total_ms = total_ms;
    let ramp = run_trial(&model, ramp_lambda).rows;
    let stable = run_trial(&model, move |_| stable_lam).rows;

    let collapse_tick = |rows: &[Row]| -> usize {
        // first tick where goodput < half of what a healthy system should serve, sustained 5 ticks
        for i in 0..rows.len() {
            let target = ramp_lambda(rows[i].t).min(nom_cap);
            if rows[i].goodput < 0.5 * target {
                let stuck = (i..rows.len().min(i + 5)).all(|j| rows[j].goodput < 0.5 * ramp_lambda(rows[j].t).min(nom_cap));
                if stuck {
                    return i;
                }
            }
        }
        rows.len()
    };

    let w_us = w as usize;
    let analyze = |rows: &[Row], up_to: usize| -> Analysis {
        let sig: Vec<f64> = rows[..up_to].iter().map(|r| r.get(&signal_name).unwrap_or(f64::NAN)).collect();
        let resid = detrend(&sig, w_us);
        let var_s = rolling_var(&resid, w_us);
        let ar1_s = rolling_ar1(&resid, w_us);
        let lo = 2 * w_us;
        let tail = |v: &[Option<f64>]| v.get(lo..).map(<[_]>::to_vec).unwrap_or_default();
        let tau_var = kendall_tau(&tail(&var_s));
        let tau_ar1 = kendall_tau(&tail(&ar1_s));
        Analysis { var_s, tau_var, tau_ar1, lo }
    };

    let c_tick = collapse_tick(&ramp);
    let collapse_time_ms = ramp[c_tick.min(ramp.len() - 1)].t;
    let ramp_a = analyze(&ramp, c_tick);
    let stable_a = analyze(&stable, stable.len());

    // alarm when ramp variance exceeds the stable run's 95th pct, sustained 5 ticks
    let baseline95 = percentile(&stable_a.var_s, 0.95).unwrap_or(f64::INFINITY);
    // JS `null > x` compares 0 > x
    let gt = |v: Option<&Option<f64>>| v.copied().flatten().unwrap_or(0.0) > baseline95;
    let mut alarm_tick: Option<usize> = None;
    let mut i = ramp_a.lo;
    while (i as i64) < c_tick as i64 - 4 {
        if ramp_a.var_s.get(i).copied().flatten().is_some() && gt(ramp_a.var_s.get(i)) {
            if (i..i + 5).all(|j| gt(ramp_a.var_s.get(j))) {
                alarm_tick = Some(i);
                break;
            }
        }
        i += 1;
    }
    let alarm_time_ms = alarm_tick.map(|t| ramp[t].t);
    let lead_ms = alarm_time_ms.map(|a| collapse_time_ms - a);

    let gate_b_pass = ramp_a.tau_var > 0.4
        && lead_ms.is_some_and(|l| l > 0.0)
        && (ramp_a.tau_var - stable_a.tau_var) > 0.25;
    let verdict = json!({
        "signal": signal_name, "window_ticks": w, "nominal_capacity_rps": js_fixed_num(nom_cap, 0),
        "collapse_time_ms": collapse_time_ms,
        "ramp_tau_variance": js_fixed_num(ramp_a.tau_var, 3),
        "ramp_tau_ar1": js_fixed_num(ramp_a.tau_ar1, 3),
        "stable_tau_variance": js_fixed_num(stable_a.tau_var, 3),
        "stable_tau_ar1": js_fixed_num(stable_a.tau_ar1, 3),
        "alarm_time_ms": alarm_time_ms.map_or(Value::Null, |v| json!(v)),
        "lead_time_ms": lead_ms.map_or(Value::Null, |v| json!(v)),
        "gate_b_pass": gate_b_pass,
    });
    println!("{}", to_js_json_pretty(&verdict));

    // variance sparkline on the ramp (pre-collapse), collapse 'X' marked
    let ramp_ds: Vec<char> = "▁▂▃▄▅▆▇█".chars().collect();
    let vmax = ramp_a.var_s.iter().flatten().fold(1e-9f64, |m, &v| if v > m { v } else { m });
    let mut line = String::new();
    for i in ramp_a.lo..c_tick {
        line.push(match ramp_a.var_s.get(i).copied().flatten() {
            None => ' ',
            Some(v) => ramp_ds[((v / vmax) * 7.0).floor().min(7.0) as usize],
        });
    }
    println!("\n{signal_name} rolling-variance over the ramp (approaching collapse):");
    println!("|{line}X|   (X=collapse)");
    if let Some(t) = alarm_tick {
        println!(" {}^ alarm fires here", " ".repeat(t - ramp_a.lo));
    }
}
