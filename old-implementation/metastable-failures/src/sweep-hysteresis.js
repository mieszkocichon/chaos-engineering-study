// M1 hysteresis artifact: a quasi-static load sweep. Ramp the offered rate slowly
// UP (carrying state) until goodput collapses (lambda_up), then slowly DOWN until
// it recovers (lambda_down). lambda_down < lambda_up is the bistable hysteresis
// loop — the defining signature of metastability, and the Gate A deliverable.
//
//   node src/sweep-hysteresis.js --lmin 100 --lmax 900 --steps 16 --dwell 4000

import { runTrial, nominalCapacityRps } from './sim.js';

function args() {
  const a = process.argv.slice(2); const o = {};
  for (let i = 0; i < a.length; i++) {
    if (a[i].startsWith('--')) {
      const k = a[i].slice(2);
      if (i + 1 < a.length && !a[i + 1].startsWith('--')) o[k] = a[++i]; else o[k] = true;
    }
  }
  return o;
}
const o = args();
const num = (k, d) => (o[k] !== undefined ? Number(o[k]) : d);

const model = {
  capacity: num('capacity', 50),
  serviceMeanMs: num('service-ms', 100),
  queueMax: num('qmax', 500),
  attemptTimeoutMs: num('timeout', 300),
  maxAttempts: num('attempts', 30),
  retryBackoffMs: num('backoff', 30),
  tickMs: num('tick', 200),
  seed: num('seed', 1),
};
const lmin = num('lmin', 100);
const lmax = num('lmax', 900);
const steps = num('steps', 16);
const dwellMs = num('dwell', 4000);

// staircase: up lmin->lmax then down lmax->lmin, each level held for dwellMs
const levelsUp = [];
const levelsDown = [];
for (let i = 0; i < steps; i++) levelsUp.push(lmin + ((lmax - lmin) * i) / (steps - 1));
for (let i = steps - 1; i >= 0; i--) levelsDown.push(lmin + ((lmax - lmin) * i) / (steps - 1));
const schedule = [...levelsUp, ...levelsDown];
const totalMs = schedule.length * dwellMs;
const lambda = (t) => {
  const idx = Math.min(schedule.length - 1, Math.floor(t / dwellMs));
  return schedule[idx];
};

const { rows } = runTrial({ ...model, lambda, totalMs });

// mean goodput over the last 40% of each dwell (steady part)
function levelGoodput(levelIndex) {
  const lo = levelIndex * dwellMs + 0.6 * dwellMs;
  const hi = (levelIndex + 1) * dwellMs;
  const w = rows.filter((r) => r.t > lo && r.t <= hi);
  return w.length ? w.reduce((s, r) => s + r.goodput, 0) / w.length : 0;
}

const nomCap = nominalCapacityRps(model);
// "collapsed" is relative to what a healthy system *should* serve at this rate:
// healthy goodput ~ min(lambda, capacity). Collapsed = serving < half of that.
const healthyTarget = (lam) => Math.min(lam, nomCap);
const isCollapsed = (p) => p.goodput < 0.5 * healthyTarget(p.lambda);

const up = levelsUp.map((lam, i) => ({ lambda: Math.round(lam), goodput: Math.round(levelGoodput(i)) }));
const down = levelsDown.map((lam, i) => ({ lambda: Math.round(lam), goodput: Math.round(levelGoodput(steps + i)) }));

// lambda_up: first (lowest) up-level whose goodput has collapsed
let lambda_up = null;
for (const p of up) if (isCollapsed(p)) { lambda_up = p.lambda; break; }
// lambda_down: scanning down-leg high->low, first level that has recovered
let lambda_down = null;
for (const p of down) if (!isCollapsed(p)) { lambda_down = p.lambda; break; }

const width = lambda_up != null && lambda_down != null ? lambda_up - lambda_down : null;

const result = {
  nominal_capacity_rps: Number(nomCap.toFixed(1)),
  lambda_up,
  lambda_down,
  hysteresis_width: width,
  bistable: width != null && width > 0,
  up_leg: up,
  down_leg: down,
};

if (o.json) { console.log(JSON.stringify(result, null, 2)); process.exit(0); }

console.log(JSON.stringify({
  nominal_capacity_rps: result.nominal_capacity_rps,
  lambda_up, lambda_down, hysteresis_width: width, bistable: result.bistable,
}, null, 2));
// ASCII hysteresis loop: goodput vs lambda, U=up-leg D=down-leg
const allG = [...up, ...down].map((p) => p.goodput);
const maxG = Math.max(1, ...allG);
const H = 12;
console.log('\ngoodput vs offered-rate (U=up-leg, D=down-leg):');
for (let row = H; row >= 0; row--) {
  const thr = (maxG * row) / H;
  let line = '';
  for (let i = 0; i < up.length; i++) {
    const u = up[i].goodput >= thr;
    const d = down[down.length - 1 - i].goodput >= thr; // align by lambda
    line += u && d ? '#' : u ? 'U' : d ? 'D' : ' ';
  }
  console.log(String(Math.round(thr)).padStart(5) + ' |' + line);
}
console.log('      +' + '-'.repeat(up.length));
console.log('       ' + up.map((p) => (p.lambda / 100 | 0)).join('') + '  (x100 rps)');
