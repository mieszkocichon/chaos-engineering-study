// T037: CircuitBreaker, RetryPolicy, ConcurrencyLimiter, MetricsCollector with a scripted clock.
import { writeFileSync } from 'fs';
import { CircuitBreaker } from '../../../../../old-implementation/when-gossip-goes-silent/lib/circuit-breaker.js';
import { RetryPolicy } from '../../../../../old-implementation/when-gossip-goes-silent/lib/retry.js';
import { ConcurrencyLimiter } from '../../../../../old-implementation/when-gossip-goes-silent/lib/concurrency-limiter.js';
import { MetricsCollector } from '../../../../../old-implementation/when-gossip-goes-silent/lib/metrics-collector.js';
import { mulberry32 } from '../../../../../old-implementation/metastable-failures/src/sim.js';

let clock = 1_700_000_000_000;
Date.now = () => clock;
const delays = [];
globalThis.setTimeout = (fn, ms) => { delays.push(ms); fn(); return 0; };

// ---- CircuitBreaker: random call/clock sequences ----
const cb = [];
for (let s = 1; s <= 20; s++) {
  const rnd = mulberry32(s);
  clock = 1_700_000_000_000;
  const threshold = 2 + Math.floor(rnd() * 6), timeout = 1000 + Math.floor(rnd() * 9000);
  const breaker = new CircuitBreaker({ failureThreshold: threshold, openTimeoutMs: timeout, name: `ds-${s}` });
  const steps = [];
  for (let i = 0; i < 200; i++) {
    clock += Math.floor(rnd() * 2500);
    const op = rnd();
    const step = { t: clock };
    if (op < 0.05) {
      const p = { failureThreshold: 2 + Math.floor(rnd() * 8), openTimeoutMs: 2000 + Math.floor(rnd() * 8000) };
      breaker.updateParams(p); step.update = p;
    } else {
      const ok = rnd() < 0.55;
      step.ok = ok;
      try { await breaker.call(async () => { if (!ok) throw new Error('boom'); return 1; }); step.result = 'ok'; }
      catch (e) { step.result = e.message; }
    }
    const { stateHistory, ...rest } = breaker.getStats();
    step.stats = { ...rest, historyLen: stateHistory.length, lastTransition: stateHistory.at(-1) ?? null };
    step.params = { failureThreshold: breaker.failureThreshold, openTimeoutMs: breaker.openTimeoutMs };
    steps.push(step);
  }
  cb.push({ threshold, timeout, name: `ds-${s}`, start: 1_700_000_000_000, steps });
}

// ---- RetryPolicy: outcome scripts ----
const retry = [];
const scripts = [
  ['err', 'err', 'ok'], ['ok'], ['err', 'err', 'err', 'err', 'err'], ['cb'], ['err', 'cb', 'ok'],
  ['err', 'err', 'err', 'err', 'err', 'err', 'ok'], ['err', 'ok'],
];
for (const params of [
  { maxRetries: 3, baseDelayMs: 200, backoffMultiplier: 2 },
  { maxRetries: 0, baseDelayMs: 200, backoffMultiplier: 2 },
  { maxRetries: 5, baseDelayMs: 500, backoffMultiplier: 3.5, maxDelayMs: 4000 },
  { maxRetries: 2, baseDelayMs: 200, backoffMultiplier: 1.5 },
]) {
  for (const script of scripts) {
    const rp = new RetryPolicy(params);
    delays.length = 0;
    let k = 0; const attempts = [];
    let result;
    try {
      result = await rp.execute(async (attempt) => {
        attempts.push(attempt);
        const o = script[Math.min(k++, script.length - 1)];
        if (o === 'ok') return `ok@${attempt}`;
        throw new Error(o === 'cb' ? 'CircuitBreakerOpen:x' : `HTTP 502: fail ${attempt}`);
      });
    } catch (e) { result = `error:${e.message}`; }
    retry.push({ params, script, result, attempts, delays: [...delays], stats: rp.getStats() });
  }
}

// ---- ConcurrencyLimiter ----
const lim = new ConcurrencyLimiter({ maxConcurrent: 3 });
const limiter = [];
const r = mulberry32(99);
for (let i = 0; i < 300; i++) {
  const op = r();
  const step = {};
  if (op < 0.5) { step.op = 'acquire'; try { await lim.acquire(); step.result = 'ok'; } catch (e) { step.result = e.message; } }
  else if (op < 0.9) { step.op = 'release'; lim.release(); }
  else { step.op = 'update'; step.max = 1 + Math.floor(r() * 5); lim.updateParams({ maxConcurrent: step.max }); }
  step.stats = lim.getStats();
  limiter.push(step);
}

// ---- MetricsCollector ----
const metrics = [];
for (let s = 1; s <= 5; s++) {
  const rnd = mulberry32(100 + s);
  clock = 1_700_000_000_000 + s * 137;
  const m = new MetricsCollector();
  const steps = [];
  for (let i = 0; i < 400; i++) {
    clock += Math.floor(rnd() * (s === 5 ? 3000 : 400));
    const op = rnd();
    const step = { t: clock };
    if (op < 0.15) { step.op = 'start'; m.startRequest(); }
    else if (op < 0.85) { step.op = 'record'; step.latencyMs = Math.floor(rnd() * 5000); step.success = rnd() < 0.8; m.recordRequest(step); }
    else if (op < 0.99) { step.op = 'snapshot'; }
    else { step.op = 'reset'; m.reset(); }
    step.snapshot = m.getSnapshot();
    step.health = m.getHealthSummary();
    steps.push(step);
  }
  metrics.push({ steps });
}

writeFileSync(new URL('../resilience.json', import.meta.url), JSON.stringify({ cb, retry, limiter, metrics }) + '\n');
