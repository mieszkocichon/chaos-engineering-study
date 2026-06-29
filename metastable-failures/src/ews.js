// Critical-slowing-down early-warning indicators, computed ONLINE from a telemetry
// stream (trailing window only — no lookahead, deployable). As a system nears a
// saddle-node bifurcation its recovery rate -> 0, so fluctuations grow (rising
// variance) and become longer-lived (rising lag-1 autocorrelation). These are the
// model-free precursors the incumbents' CTMC line does not use.

// trailing rolling mean
export function rollingMean(xs, w) {
  const out = new Array(xs.length).fill(null);
  let sum = 0;
  for (let i = 0; i < xs.length; i++) {
    sum += xs[i];
    if (i >= w) sum -= xs[i - w];
    if (i >= w - 1) out[i] = sum / w;
  }
  return out;
}

// detrend by subtracting the trailing rolling mean (removes the slow drift so the
// indicators reflect fluctuations, not the trend itself)
export function detrend(xs, w) {
  const m = rollingMean(xs, w);
  return xs.map((x, i) => (m[i] == null ? null : x - m[i]));
}

// trailing rolling variance of a (possibly detrended) series
export function rollingVar(xs, w) {
  const out = new Array(xs.length).fill(null);
  for (let i = w - 1; i < xs.length; i++) {
    let n = 0, mean = 0, m2 = 0;
    for (let j = i - w + 1; j <= i; j++) {
      const v = xs[j];
      if (v == null) { n = 0; break; }
      n++; const d = v - mean; mean += d / n; m2 += d * (v - mean);
    }
    out[i] = n > 1 ? m2 / (n - 1) : null;
  }
  return out;
}

// trailing rolling lag-1 autocorrelation (AR(1) coefficient) of a series
export function rollingAR1(xs, w) {
  const out = new Array(xs.length).fill(null);
  for (let i = w - 1; i < xs.length; i++) {
    const seg = xs.slice(i - w + 1, i + 1);
    if (seg.some((v) => v == null)) continue;
    const n = seg.length;
    const mean = seg.reduce((s, v) => s + v, 0) / n;
    let num = 0, den = 0;
    for (let k = 0; k < n; k++) den += (seg[k] - mean) ** 2;
    for (let k = 1; k < n; k++) num += (seg[k] - mean) * (seg[k - 1] - mean);
    out[i] = den > 0 ? num / den : null;
  }
  return out;
}

// Kendall's tau of a series against time index (trend test). Returns tau in [-1,1].
export function kendallTau(ys) {
  const xs = ys.filter((v) => v != null);
  const n = xs.length;
  if (n < 3) return 0;
  let conc = 0, disc = 0;
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      const d = xs[j] - xs[i];
      if (d > 0) conc++; else if (d < 0) disc++;
    }
  }
  return (conc - disc) / (0.5 * n * (n - 1));
}

// percentile of a numeric array (nearest-rank)
export function percentile(xs, p) {
  const a = xs.filter((v) => v != null).slice().sort((u, v) => u - v);
  if (!a.length) return null;
  const r = Math.min(a.length - 1, Math.max(0, Math.round(p * (a.length - 1))));
  return a[r];
}
