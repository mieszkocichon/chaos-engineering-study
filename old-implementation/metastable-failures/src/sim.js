// M1 model layer: a discrete-event simulation of the retry-amplification loop —
// the canonical sustaining effect behind metastable congestive collapse
// (OSDI'22 calls it a "retry storm"). This is the *abstract* layer, in the spirit
// of the incumbents' CTMC/DES (HotOS'25); real network injection on the ChaosMAS
// services comes in later milestones. Its only job here is to prove Gate A:
// metastability is inducible on demand, with measurable hysteresis.
//
// The sustaining effect has three coupled parts:
//   1. clients time out and RETRY  -> extra offered load (amplification)
//   2. the server keeps doing WASTED work on requests the client already abandoned
//   3. shedding triggers fast retries -> more amplification
// Together these can keep the server saturated on doomed work even after the
// trigger is gone -> the system stays stuck (metastable).

// ---- deterministic RNG (mulberry32): one seed => bit-identical run ----
export function mulberry32(seed) {
  let a = seed >>> 0;
  return function () {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const expRand = (rng, mean) => -Math.log(1 - rng()) * mean;

// ---- tiny binary min-heap keyed by (time, seq) ----
class Heap {
  constructor() { this.a = []; }
  get size() { return this.a.length; }
  push(e) {
    const a = this.a; a.push(e); let i = a.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (a[p].time < a[i].time || (a[p].time === a[i].time && a[p].seq <= a[i].seq)) break;
      [a[p], a[i]] = [a[i], a[p]]; i = p;
    }
  }
  pop() {
    const a = this.a, top = a[0], last = a.pop();
    if (a.length) {
      a[0] = last; let i = 0;
      for (;;) {
        const l = 2 * i + 1, r = l + 1; let s = i;
        if (l < a.length && (a[l].time < a[s].time || (a[l].time === a[s].time && a[l].seq < a[s].seq))) s = l;
        if (r < a.length && (a[r].time < a[s].time || (a[r].time === a[s].time && a[r].seq < a[s].seq))) s = r;
        if (s === i) break;
        [a[s], a[i]] = [a[i], a[s]]; i = s;
      }
    }
    return top;
  }
}

// lambda(t) in requests/sec: baseline, with a surge window [surgeStartMs, +surgeDurMs)
export function surgeSchedule({ baseline, surge, surgeStartMs, surgeDurMs }) {
  return (t) => (t >= surgeStartMs && t < surgeStartMs + surgeDurMs ? surge : baseline);
}

/**
 * Run one retry-storm trial. Returns per-tick telemetry + summary counters.
 * Times are milliseconds of virtual time.
 */
export function runTrial(cfg) {
  const {
    capacity = 50,            // concurrent service slots (server capacity)
    serviceMeanMs = 100,      // mean service time (exponential)
    queueMax = 200,           // bounded queue; beyond this -> shed
    attemptTimeoutMs = 500,   // client per-attempt timeout
    maxAttempts = 5,          // attempts per job (1 original + retries)
    retryBackoffMs = 50,      // delay before a retry / after a shed
    cancelQueuedOnTimeout = false, // if false, abandoned queued work still runs => wasted (stronger metastability)
    lambda,                   // (t)=>rate req/s
    totalMs = 60000,
    tickMs = 200,
    seed = 1,
  } = cfg;

  const rng = mulberry32(seed);
  const ev = new Heap();
  let seq = 0;
  const at = (time, type, payload) => ev.push({ time, seq: seq++, type, payload });

  let now = 0;
  let inService = 0;
  const queue = []; // FIFO of attempts waiting for a slot
  let nextJobId = 0;

  // counters for the current tick
  let tickEnd = tickMs;
  const rows = [];
  let c = freshCounters();
  function freshCounters() {
    return { fresh: 0, retry: 0, offered: 0, success: 0, wasted: 0, shed: 0, timeout: 0 };
  }
  function emitTick() {
    rows.push({
      t: tickEnd,
      goodput: c.success / (tickMs / 1000),     // successful, client-still-wanted completions /s
      offered: c.offered / (tickMs / 1000),     // all submitted attempts /s (fresh+retry)
      fresh: c.fresh / (tickMs / 1000),
      retry: c.retry / (tickMs / 1000),
      wasted: c.wasted / (tickMs / 1000),
      shed: c.shed / (tickMs / 1000),
      timeout: c.timeout / (tickMs / 1000),
      queueLen: queue.length,
      inService,
    });
    c = freshCounters();
  }
  function advanceTo(t) {
    while (t >= tickEnd) { emitTick(); tickEnd += tickMs; }
  }

  // ---- job / attempt model ----
  const jobs = new Map(); // id -> { attempts, succeeded }
  function submitAttempt(jobId, isRetry) {
    const job = jobs.get(jobId);
    job.attempts++;
    c.offered++; if (isRetry) c.retry++; else c.fresh++;
    const a = { jobId, state: 'new', arrival: now, timeoutAt: now + attemptTimeoutMs };
    at(a.timeoutAt, 'TIMEOUT', a);
    if (inService < capacity) {
      startService(a);
    } else if (queue.length < queueMax) {
      a.state = 'queued'; queue.push(a);
    } else {
      // shed: immediate reject -> fast retry (amplification)
      a.state = 'rejected'; c.shed++;
      maybeRetry(job, jobId);
    }
  }
  function startService(a) {
    a.state = 'serving'; inService++;
    at(now + expRand(rng, serviceMeanMs), 'DEPART', a);
  }
  function maybeRetry(job, jobId) {
    if (!job.succeeded && job.attempts < maxAttempts) {
      at(now + retryBackoffMs, 'RETRY', { jobId });
    }
  }

  // first fresh arrival
  at(0, 'FRESH', null);

  while (ev.size) {
    const e = ev.pop();
    if (e.time > totalMs) break;
    now = e.time; advanceTo(now);

    if (e.type === 'FRESH') {
      const id = nextJobId++;
      jobs.set(id, { attempts: 0, succeeded: false });
      submitAttempt(id, false);
      const rate = Math.max(1e-9, lambda(now)); // req/s
      at(now + expRand(rng, 1000 / rate), 'FRESH', null);
    } else if (e.type === 'RETRY') {
      const job = jobs.get(e.payload.jobId);
      if (job && !job.succeeded) submitAttempt(e.payload.jobId, true);
    } else if (e.type === 'DEPART') {
      const a = e.payload; inService--;
      const job = jobs.get(a.jobId);
      if (now <= a.timeoutAt && job && !job.succeeded) {
        job.succeeded = true; c.success++;       // goodput: completed before client gave up
      } else {
        c.wasted++;                               // client already abandoned this attempt
      }
      // pull next queued attempt into service
      while (queue.length) {
        const next = queue.shift();
        if (next.state === 'queued') { startService(next); break; }
      }
    } else if (e.type === 'TIMEOUT') {
      const a = e.payload;
      if (a.state === 'done' || a.state === 'rejected') continue;
      c.timeout++;
      const job = jobs.get(a.jobId);
      if (a.state === 'queued' && cancelQueuedOnTimeout) a.state = 'canceled';
      // (if serving, it keeps running and becomes wasted work on DEPART)
      if (job) maybeRetry(job, a.jobId);
    }
  }
  advanceTo(totalMs);
  return { rows, cfg: { capacity, serviceMeanMs, queueMax, attemptTimeoutMs, maxAttempts, retryBackoffMs, cancelQueuedOnTimeout, totalMs, tickMs, seed } };
}

// nominal capacity in req/s (for reference): slots / mean service time
export const nominalCapacityRps = (cfg) => cfg.capacity / (cfg.serviceMeanMs / 1000);
