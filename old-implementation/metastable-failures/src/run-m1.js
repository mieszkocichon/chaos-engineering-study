// M1 Gate A probe: transient recovery test.
// Warm up to steady state -> inject a load surge -> REMOVE it -> does goodput
// return to baseline (recovered) or stay collapsed (metastable / stuck)?
// A "stuck" outcome with the trigger gone is the signature we need.
//
//   node src/run-m1.js --baseline 300 --surge 1500 --surge-dur 10000 [--json]

import { runTrial, surgeSchedule, nominalCapacityRps } from './sim.js';

function args() {
  const a = process.argv.slice(2); const o = {};
  for (let i = 0; i < a.length; i++) {
    if (a[i].startsWith('--')) {
      const k = a[i].slice(2);
      if (i + 1 < a.length && !a[i + 1].startsWith('--')) { o[k] = a[++i]; } else { o[k] = true; }
    }
  }
  return o;
}
const o = args();
const num = (k, d) => (o[k] !== undefined ? Number(o[k]) : d);

const base = {
  capacity: num('capacity', 50),
  serviceMeanMs: num('service-ms', 100),
  queueMax: num('qmax', 200),
  attemptTimeoutMs: num('timeout', 500),
  maxAttempts: num('attempts', 5),
  retryBackoffMs: num('backoff', 50),
  cancelQueuedOnTimeout: o['cancel-queued'] === 'true' || o['cancel-queued'] === true,
  totalMs: num('total', 60000),
  tickMs: num('tick', 200),
  seed: num('seed', 1),
};
const baseline = num('baseline', 300);
const surge = num('surge', 1500);
const surgeStartMs = num('surge-start', 15000);
const surgeDurMs = num('surge-dur', 10000);
const settleMs = num('settle', 10000);

const lambda = surgeSchedule({ baseline, surge, surgeStartMs, surgeDurMs });
const { rows } = runTrial({ ...base, lambda });

const mean = (xs) => (xs.length ? xs.reduce((s, x) => s + x, 0) / xs.length : 0);
const window = (lo, hi) => rows.filter((r) => r.t > lo && r.t <= hi);
const gp = (rs) => mean(rs.map((r) => r.goodput));

const surgeEnd = surgeStartMs + surgeDurMs;
const baselineGoodput = gp(window(5000, surgeStartMs));
const surgeGoodput = gp(window(surgeStartMs, surgeEnd));
const postGoodput = gp(window(surgeEnd + settleMs, base.totalMs));
const recovered = postGoodput >= 0.9 * baselineGoodput;
const collapseRatio = baselineGoodput > 0 ? postGoodput / baselineGoodput : 1;

const nomCap = nominalCapacityRps(base);
const summary = {
  nominal_capacity_rps: Number(nomCap.toFixed(1)),
  baseline_rps: baseline,
  surge_rps: surge,
  baseline_goodput: Number(baselineGoodput.toFixed(1)),
  surge_goodput: Number(surgeGoodput.toFixed(1)),
  post_goodput: Number(postGoodput.toFixed(1)),
  post_over_baseline: Number(collapseRatio.toFixed(3)),
  recovered,
  metastable_stuck: !recovered,
};

if (o.json) {
  console.log(JSON.stringify({ summary, rows }, null, 2));
} else {
  console.log(JSON.stringify(summary, null, 2));
  // compact goodput sparkline over time
  const maxG = Math.max(1, ...rows.map((r) => r.goodput));
  const ramp = '▁▂▃▄▅▆▇█';
  const spark = rows
    .filter((_, i) => i % Math.ceil(rows.length / 80) === 0)
    .map((r) => ramp[Math.min(7, Math.floor((r.goodput / maxG) * 7))])
    .join('');
  console.log('goodput |' + spark + '|  (surge window in the middle)');
}
