//! Critical-slowing-down early-warning indicators, 1:1 port of `metastable-failures/src/ews.js`.
//! Trailing windows only (no lookahead). `None` plays the role of JS `null`.

use crate::js::js_round;

/// Trailing rolling mean (input has no gaps, as in run-m2).
pub fn rolling_mean(xs: &[f64], w: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; xs.len()];
    let mut sum = 0.0;
    for i in 0..xs.len() {
        sum += xs[i];
        if i >= w {
            sum -= xs[i - w];
        }
        if i + 1 >= w {
            out[i] = Some(sum / w as f64);
        }
    }
    out
}

/// Subtract the trailing rolling mean.
pub fn detrend(xs: &[f64], w: usize) -> Vec<Option<f64>> {
    let m = rolling_mean(xs, w);
    xs.iter().zip(m).map(|(x, m)| m.map(|m| x - m)).collect()
}

/// Trailing rolling sample variance (Welford); any gap in the window gives `None`.
pub fn rolling_var(xs: &[Option<f64>], w: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; xs.len()];
    if w == 0 {
        return out;
    }
    for i in (w - 1)..xs.len() {
        let (mut n, mut mean, mut m2) = (0u32, 0.0, 0.0);
        for v in &xs[i + 1 - w..=i] {
            let Some(v) = *v else {
                n = 0;
                break;
            };
            n += 1;
            let d = v - mean;
            mean += d / n as f64;
            m2 += d * (v - mean);
        }
        out[i] = if n > 1 { Some(m2 / (n - 1) as f64) } else { None };
    }
    out
}

/// Trailing rolling lag-1 autocorrelation.
pub fn rolling_ar1(xs: &[Option<f64>], w: usize) -> Vec<Option<f64>> {
    let mut out = vec![None; xs.len()];
    if w == 0 {
        return out;
    }
    for i in (w - 1)..xs.len() {
        let seg: Option<Vec<f64>> = xs[i + 1 - w..=i].iter().copied().collect();
        let Some(seg) = seg else { continue };
        let n = seg.len();
        let mean = seg.iter().fold(0.0, |s, v| s + v) / n as f64;
        let (mut num, mut den) = (0.0, 0.0);
        for v in &seg {
            den += (v - mean).powi(2);
        }
        for k in 1..n {
            num += (seg[k] - mean) * (seg[k - 1] - mean);
        }
        out[i] = if den > 0.0 { Some(num / den) } else { None };
    }
    out
}

/// Kendall's tau of a series against its index; gaps are dropped first.
pub fn kendall_tau(ys: &[Option<f64>]) -> f64 {
    let xs: Vec<f64> = ys.iter().flatten().copied().collect();
    let n = xs.len();
    if n < 3 {
        return 0.0;
    }
    let (mut conc, mut disc) = (0i64, 0i64);
    for i in 0..n {
        for j in i + 1..n {
            let d = xs[j] - xs[i];
            if d > 0.0 {
                conc += 1;
            } else if d < 0.0 {
                disc += 1;
            }
        }
    }
    (conc - disc) as f64 / (0.5 * n as f64 * (n as f64 - 1.0))
}

/// Nearest-rank percentile; gaps dropped; empty → `None`.
pub fn percentile(xs: &[Option<f64>], p: f64) -> Option<f64> {
    let mut a: Vec<f64> = xs.iter().flatten().copied().collect();
    if a.is_empty() {
        return None;
    }
    a.sort_by(|u, v| u.partial_cmp(v).unwrap());
    let r = js_round(p * (a.len() as f64 - 1.0)).max(0.0).min(a.len() as f64 - 1.0);
    Some(a[r as usize])
}
