// M2 Gate B: does a critical-slowing-down precursor appear BEFORE collapse?
// Gradually ramp offered load toward the tipping point lambda_up and watch the
// rolling variance + lag-1 autocorrelation of a state variable (queue length).
// Compare against a matched STABLE run held safely below lambda_up.
// Gate B passes if the indicators rise significantly pre-collapse on the ramp
// (Kendall-tau > 0) while staying flat on the stable run, with non-zero lead time.
//
//   node src/run-m2.js --signal queueLen --window 25

import { runTrial, nominalCapacityRps } from './sim.js';
import { detrend, rollingVar, rollingAR1, kendallTau, percentile } from './ews.js';

function args() {
  const a = process.argv.slice(2); const o = {};
  for (let i = 0; i < a.length; i++) {
    if (a[i].startsWith('--')) { const k = a[i].slice(2); if (i + 1 < a.length && !a[i + 1].startsWith('--')) o[k] = a[++i]; else o[k] = true; }
  }
  return o;
}
const o = args();
const num = (k, d) => (o[k] !== undefined ? Number(o[k]) : d);

const model = {
  capacity: num('capacity', 50), serviceMeanMs: num('service-ms', 100),
  queueMax: num('qmax', 500), attemptTimeoutMs: num('timeout', 300),
  maxAttempts: num('attempts', 3), retryBackoffMs: num('backoff', 30),
  tickMs: num('tick', 200), seed: num('seed', 1),
};
const W = num('window', 25);                 // rolling window (ticks); 25*200ms = 5s
const signalName = o.signal || 'queueLen';
const nomCap = nominalCapacityRps(model);

const l0 = num('l0', 200);                    // ramp start (healthy)
const l1 = num('l1', 560);                    // ramp end (past lambda_up)
const rampMs = num('ramp', 60000);
const totalMs = rampMs + num('hold', 12000);
const rampLambda = (t) => (t >= rampMs ? l1 : l0 + ((l1 - l0) * t) / rampMs);
const stableLam = num('stable', 260);         // matched stable load (clearly in the healthy basin)
const stableLambda = () => stableLam;

function series(rows, name) { return rows.map((r) => r[name]); }
function collapseTick(rows) {
  // first tick where goodput < half of what a healthy system should serve, sustained
  for (let i = 0; i < rows.length; i++) {
    const lam = rampLambda(rows[i].t);
    const target = Math.min(lam, nomCap);
    if (rows[i].goodput < 0.5 * target) {
      // require it to stay collapsed for 5 ticks (avoid transient dips)
      let stuck = true;
      for (let j = i; j < Math.min(rows.length, i + 5); j++) {
        const t2 = Math.min(rampLambda(rows[j].t), nomCap);
        if (rows[j].goodput >= 0.5 * t2) { stuck = false; break; }
      }
      if (stuck) return i;
    }
  }
  return rows.length;
}

function analyze(rows, sigName, upToTick) {
  const sig = series(rows.slice(0, upToTick), sigName);
  const resid = detrend(sig, W);
  const varS = rollingVar(resid, W);
  const ar1S = rollingAR1(resid, W);
  // analysis window: skip warmup (2W) to the end of the provided range
  const lo = 2 * W;
  const varWin = varS.slice(lo);
  const ar1Win = ar1S.slice(lo);
  return { varS, ar1S, tauVar: kendallTau(varWin), tauAR1: kendallTau(ar1Win), lo };
}

// ----- run both scenarios -----
const ramp = runTrial({ ...model, lambda: rampLambda, totalMs });
const stable = runTrial({ ...model, lambda: stableLambda, totalMs });

const cTick = collapseTick(ramp.rows);
const collapseTimeMs = ramp.rows[Math.min(cTick, ramp.rows.length - 1)].t;

const rampA = analyze(ramp.rows, signalName, cTick);      // pre-collapse only
const stableA = analyze(stable.rows, signalName, stable.rows.length);

// lead time: alarm when ramp variance exceeds stable run's 95th pct, sustained 5 ticks
const baseline95 = percentile(stableA.varS, 0.95) ?? Infinity;
let alarmTick = null;
for (let i = rampA.lo; i < cTick - 4; i++) {
  if (rampA.varS[i] != null && rampA.varS[i] > baseline95) {
    let sustained = true;
    for (let j = i; j < i + 5; j++) if (!(rampA.varS[j] > baseline95)) { sustained = false; break; }
    if (sustained) { alarmTick = i; break; }
  }
}
const alarmTimeMs = alarmTick != null ? ramp.rows[alarmTick].t : null;
const leadMs = alarmTimeMs != null ? collapseTimeMs - alarmTimeMs : null;

const verdict = {
  signal: signalName, window_ticks: W, nominal_capacity_rps: Number(nomCap.toFixed(0)),
  collapse_time_ms: collapseTimeMs,
  ramp_tau_variance: Number(rampA.tauVar.toFixed(3)),
  ramp_tau_ar1: Number(rampA.tauAR1.toFixed(3)),
  stable_tau_variance: Number(stableA.tauVar.toFixed(3)),
  stable_tau_ar1: Number(stableA.tauAR1.toFixed(3)),
  alarm_time_ms: alarmTimeMs,
  lead_time_ms: leadMs,
  // Variance is the primary, discriminative CSD indicator here; AR(1) is reported
  // transparently but is ill-conditioned on the near-constant stable signal.
  gate_b_pass: rampA.tauVar > 0.4 && leadMs != null && leadMs > 0 && (rampA.tauVar - stableA.tauVar) > 0.25,
};
console.log(JSON.stringify(verdict, null, 2));

// variance sparkline on the ramp (pre-collapse), alarm '!' and collapse 'X' marked
const ramp_ds = '▁▂▃▄▅▆▇█';
const vmax = Math.max(1e-9, ...rampA.varS.filter((v) => v != null));
let line = '';
for (let i = rampA.lo; i < cTick; i++) {
  const v = rampA.varS[i];
  line += v == null ? ' ' : ramp_ds[Math.min(7, Math.floor((v / vmax) * 7))];
}
console.log(`\n${signalName} rolling-variance over the ramp (approaching collapse):`);
console.log('|' + line + 'X|   (X=collapse)');
if (alarmTick != null) console.log(' ' + ' '.repeat(alarmTick - rampA.lo) + '^ alarm fires here');
