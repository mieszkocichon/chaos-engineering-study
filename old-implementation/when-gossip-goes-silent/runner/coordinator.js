// Runs one experiment cell (strategy × fault-profile × seeds) in a single cluster
// lifecycle — soft-resetting in-process state between seeds via /experiment/reset
// instead of tearing down containers. Cuts ~45s docker-compose overhead per seed.
//
// Env vars: TOPOLOGY, STRATEGY, FAULT_BASE, FAULT_PROFILE, FAULT_PARTITION, SEEDS,
//           RPS, DURATION_SEC, WARMUP_SEC, BASE_PORT, RESULTS_DIR, HEALTH_TIMEOUT_MS

import { writeFileSync, readFileSync, unlinkSync, mkdirSync } from 'fs';
import { spawn } from 'child_process';
import { promises as dns } from 'dns';
import { loadTopology, loadFaultProfile } from '../lib/topology-loader.js';
import { CentralController } from '../lib/strategies/central.js';

const TOPOLOGY = process.env.TOPOLOGY || 'ring-50';
const FAULT_BASE = process.env.FAULT_BASE || 'no-wan';
const FAULT_PROFILE = process.env.FAULT_PROFILE || 'baseline-flaky';
const FAULT_PARTITION = process.env.FAULT_PARTITION || '';
const STRATEGY = process.env.STRATEGY || 'none';
const SEEDS_STR = process.env.SEEDS || '1000,2000,3000,4000,5000';
const SEEDS = SEEDS_STR.split(',').map(s => parseInt(s.trim(), 10)).filter(Number.isFinite);
const RPS = parseInt(process.env.RPS || '20', 10);
const DURATION_SEC = parseInt(process.env.DURATION_SEC || '60', 10);
const WARMUP_SEC = parseInt(process.env.WARMUP_SEC || '15', 10);
const BASE_PORT = parseInt(process.env.BASE_PORT || '4000', 10);
const RESULTS_DIR = process.env.RESULTS_DIR || '/results';
const HEALTH_TIMEOUT_MS = parseInt(process.env.HEALTH_TIMEOUT_MS || '300000', 10);
const INTER_SEED_SETTLE_MS = parseInt(process.env.INTER_SEED_SETTLE_MS || '2000', 10);

if (SEEDS.length === 0) {
  console.error('[coordinator] SEEDS env var is empty or invalid');
  process.exit(1);
}

const dockerHostResolver = (id) => id;
const topology = loadTopology(TOPOLOGY, { basePort: BASE_PORT, hostResolver: dockerHostResolver });

console.log(`[coordinator] topology=${TOPOLOGY} strategy=${STRATEGY} base=${FAULT_BASE} fault=${FAULT_PROFILE} partition=${FAULT_PARTITION || '(none)'} seeds=[${SEEDS.join(',')}]`);
console.log(`[coordinator] Services: ${topology.services.length} (entry: ${topology.entryPoint?.id})`);

async function fetchWithTimeout(url, options = {}, timeoutMs = 5000) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    return await fetch(url, { ...options, signal: ctrl.signal });
  } finally {
    clearTimeout(timer);
  }
}

async function waitForHealthy(services, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  const pending = new Set(services.map(s => s.id));
  console.log(`[coordinator] Waiting for ${pending.size} services (timeout: ${timeoutMs}ms)...`);

  while (pending.size > 0 && Date.now() < deadline) {
    const checks = [...pending].map(async (id) => {
      const svc = services.find(s => s.id === id);
      const url = `http://${dockerHostResolver(id)}:${svc.port}/health`;
      try {
        const res = await fetchWithTimeout(url, {}, 2000);
        if (res.ok) return id;
      } catch { /* keep waiting */ }
      return null;
    });
    const ready = (await Promise.all(checks)).filter(Boolean);
    for (const id of ready) {
      pending.delete(id);
      console.log(`[coordinator] ${id} healthy (${pending.size} remaining)`);
    }
    if (pending.size > 0) await new Promise(r => setTimeout(r, 1000));
  }
  if (pending.size > 0) {
    throw new Error(`[coordinator] Health check timeout. Not ready: ${[...pending].join(', ')}`);
  }
  console.log('[coordinator] All services healthy');
}

async function broadcastStart(services, experimentStartTime) {
  console.log(`[coordinator] Broadcasting experimentStartTime=${experimentStartTime}`);
  await Promise.all(services.map(async (svc) => {
    const url = `http://${dockerHostResolver(svc.id)}:${svc.port}/start`;
    try {
      await fetchWithTimeout(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ experimentStartTime }),
      }, 5000);
    } catch (err) {
      console.warn(`[coordinator] ${svc.id} /start failed: ${err.message}`);
    }
  }));
}

// Soft reset between seeds: re-seeds PRNG, clears metrics, recovers all faults,
// and triggers strategy-internal state reset (if the strategy wired onReset).
async function broadcastReset(services, nextSeed) {
  console.log(`[coordinator] Soft reset — next seed=${nextSeed}`);
  await Promise.all(services.map(async (svc) => {
    const url = `http://${dockerHostResolver(svc.id)}:${svc.port}/experiment/reset`;
    try {
      await fetchWithTimeout(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ seed: nextSeed }),
      }, 10000);
    } catch (err) {
      console.warn(`[coordinator] ${svc.id} /experiment/reset failed: ${err.message}`);
    }
  }));
}

async function collectMetrics(services) {
  const serviceMetrics = {};
  await Promise.all(services.map(async (svc) => {
    const url = `http://${dockerHostResolver(svc.id)}:${svc.port}/metrics`;
    try {
      const res = await fetchWithTimeout(url, {}, 5000);
      const data = await res.json();
      serviceMetrics[svc.id] = {
        ...data.snapshot,
        strategyStats: data.strategyStats ?? null,
      };
    } catch (err) {
      console.warn(`[coordinator] metrics ${svc.id}: ${err.message}`);
      serviceMetrics[svc.id] = null;
    }
  }));
  return serviceMetrics;
}

// Fires a single fault at t=startTimeSec and, if endTimeSec present and the fault
// is a transient overlay (burst-failure, slow-degradation, network-partition),
// fires a type-scoped recovery at t=endTimeSec. WAN baseline and baseline-flaky
// have endTimeSec >= 999 → treated as persistent until softReset.
function scheduleSingleFault(fault, svc, targetUrl, options, experimentStart) {
  const startDelayMs = (fault.startTimeSec ?? 0) * 1000;
  setTimeout(() => {
    console.log(`[coordinator] [t=${((Date.now()-experimentStart)/1000).toFixed(1)}s] Injecting '${fault.type}' on ${svc.id}`);
    fetch(`${targetUrl}/${fault.type}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(options),
    }).catch(err => console.error(`[coordinator] Inject failed: ${err.message}`));
  }, startDelayMs);

  let recoverTimeSec = fault.endTimeSec;
  if (fault.type === 'cascade-crash' && !recoverTimeSec) recoverTimeSec = 35;

  if (recoverTimeSec !== undefined && recoverTimeSec < 999) {
    const recoverDelayMs = recoverTimeSec * 1000;
    setTimeout(() => {
      console.log(`[coordinator] [t=${((Date.now()-experimentStart)/1000).toFixed(1)}s] Recovering '${fault.type}' on ${svc.id}`);
      fetch(`${targetUrl}/recover/${fault.type}`, { method: 'POST' })
        .catch(err => console.error(`[coordinator] Recover failed: ${err.message}`));
    }, recoverDelayMs);
  }
}

// Resolves Docker service hostnames to bridge IPs (used for network-partition peerIps).
async function resolvePeerIps(peerServices) {
  const ips = [];
  for (const name of peerServices) {
    try {
      const { address } = await dns.lookup(name);
      ips.push(address);
    } catch (err) {
      console.warn(`[coordinator] DNS lookup failed for ${name}: ${err.message}`);
    }
  }
  return ips;
}

// Applies base layer (wan-50ms or no-op) at experiment start — persists across seed.
async function applyBase(services, experimentStart) {
  if (FAULT_BASE === 'no-wan' || !FAULT_BASE) return;
  const profile = loadFaultProfile(FAULT_BASE);
  for (const fault of (profile.faults || [])) {
    const svc = services.find(s => s.id === fault.targetService);
    if (!svc) continue;
    const targetUrl = `http://${dockerHostResolver(svc.id)}:${svc.port}/faults`;
    scheduleSingleFault(fault, svc, targetUrl, fault.options ?? {}, experimentStart);
  }
}

async function applyOverlay(services, experimentStart) {
  const profile = loadFaultProfile(FAULT_PROFILE);
  for (const fault of (profile.faults || [])) {
    const svc = services.find(s => s.id === fault.targetService);
    if (!svc) continue;
    const targetUrl = `http://${dockerHostResolver(svc.id)}:${svc.port}/faults`;
    scheduleSingleFault(fault, svc, targetUrl, fault.options ?? {}, experimentStart);
  }
}

async function applyPartition(services, experimentStart) {
  if (!FAULT_PARTITION) return;
  const profile = loadFaultProfile(FAULT_PARTITION);
  for (const fault of (profile.faults || [])) {
    const svc = services.find(s => s.id === fault.targetService);
    if (!svc) continue;
    const targetUrl = `http://${dockerHostResolver(svc.id)}:${svc.port}/faults`;
    // Resolve peerServices → peerIps via Docker DNS
    const peerServices = fault.peerServices || [];
    const peerIps = await resolvePeerIps(peerServices);
    const options = { ...(fault.options ?? {}), peerIps };
    console.log(`[coordinator] Partition ${svc.id} ⇔ [${peerServices.join(',')}] = [${peerIps.join(',')}]`);
    scheduleSingleFault(fault, svc, targetUrl, options, experimentStart);
  }
}

function runK6(targetUrl, rps, durationSec, warmupSec, resultsDir) {
  return new Promise((resolve, reject) => {
    const summaryFile = `${resultsDir}/_k6_summary_tmp_${Date.now()}.json`;
    const startTime = Date.now();

    console.log(`[coordinator] k6: ${rps} RPS | ${warmupSec}s warmup + ${durationSec}s measurement | target=${targetUrl}`);

    const k6 = spawn('k6', [
      'run',
      '--env', `TARGET_URL=${targetUrl}`,
      '--env', `RPS=${rps}`,
      '--env', `DURATION_SEC=${durationSec}`,
      '--env', `WARMUP_SEC=${warmupSec}`,
      '--env', `SUMMARY_FILE=${summaryFile}`,
      '/app/runner/k6-script.js',
    ]);
    k6.stdout.on('data', (d) => process.stdout.write(d));
    k6.stderr.on('data', (d) => process.stderr.write(d));
    k6.on('close', (code) => {
      const wallClockMs = Date.now() - startTime;
      if (code !== 0) return reject(new Error(`k6 exited with code ${code}`));
      try {
        const raw = JSON.parse(readFileSync(summaryFile, 'utf8'));
        try { unlinkSync(summaryFile); } catch {}
        resolve({ k6Summary: raw, wallClockMs });
      } catch (err) {
        reject(new Error(`Failed to parse k6 summary: ${err.message}`));
      }
    });
    k6.on('error', reject);
  });
}

function buildResult(workloadResult, serviceMetrics, experimentStartTime, seed, spofMeta = null) {
  const { k6Summary, wallClockMs } = workloadResult;
  const m = k6Summary.metrics;
  const dur = m.http_req_duration?.values || {};
  const reqs = m.http_reqs?.values || {};
  const failed = m.http_req_failed?.values || {};

  const totalRequests = Math.round(reqs.count || 0);
  const failureCount = Math.round((failed.rate || 0) * totalRequests);
  const successCount = totalRequests - failureCount;

  return {
    config: {
      topology: TOPOLOGY,
      strategy: STRATEGY,
      faultBase: FAULT_BASE,
      faultProfile: FAULT_PROFILE,
      faultPartition: FAULT_PARTITION || null,
      seed,
      rps: RPS,
      durationSec: DURATION_SEC,
      warmupSec: WARMUP_SEC,
      softReset: true,
      loadGenerator: 'k6',
      spof: spofMeta, // null when SPOF not enabled; {killAtSec, killedAtMs} otherwise
    },
    experimentStartTime,
    summary: {
      totalRequests,
      successCount,
      failureCount,
      successRate: totalRequests > 0 ? successCount / totalRequests : 0,
      avgLatencyMs: Math.round(dur.avg || 0),
      p50Ms: Math.round(dur['p(50)'] || dur.med || 0),
      p90Ms: Math.round(dur['p(90)'] || 0),
      p99Ms: Math.round(dur['p(99)'] || 0),
      minLatencyMs: Math.round(dur.min || 0),
      maxLatencyMs: Math.round(dur.max || 0),
      wallClockMs,
    },
    k6Metrics: m,
    serviceMetrics,
    timestamp: new Date().toISOString(),
  };
}

function resultFilename(seed) {
  const partitionTag = FAULT_PARTITION ? `+${FAULT_PARTITION}` : '';
  return `${RESULTS_DIR}/${TOPOLOGY}_${STRATEGY}_${FAULT_BASE}_${FAULT_PROFILE}${partitionTag}_${seed}_${Date.now()}.json`;
}

// ── Main experiment loop ─────────────────────────────────────────────────────
await waitForHealthy(topology.services, HEALTH_TIMEOUT_MS);
mkdirSync(RESULTS_DIR, { recursive: true });

for (let i = 0; i < SEEDS.length; i++) {
  const seed = SEEDS[i];
  const isFirstSeed = i === 0;
  const isLastSeed = i === SEEDS.length - 1;

  console.log(`\n=== Cell seed ${seed} (${i+1}/${SEEDS.length}) ===`);

  // If not the first seed, soft reset all services to pre-seed state.
  if (!isFirstSeed) {
    await broadcastReset(topology.services, seed);
    await new Promise(r => setTimeout(r, INTER_SEED_SETTLE_MS));
  }

  const experimentStartTime = Date.now();
  await broadcastStart(topology.services, experimentStartTime);

  // Base + overlay + partition scheduled relative to experimentStartTime.
  await applyBase(topology.services, experimentStartTime);
  await applyOverlay(topology.services, experimentStartTime);
  await applyPartition(topology.services, experimentStartTime);

  // Central controller restarts for every seed to isolate state across seeds.
  let centralController = null;
  let spofKillTimer = null;
  let spofKilledAtMs = null;
  // SPOF test hook: SPOF_KILL_AT_SEC (>0) stops the central controller at
  // T+<value>s from workload start, simulating coordinator death. Only active
  // for strategy=central; ignored otherwise. Records kill timestamp in result.
  const SPOF_KILL_AT_SEC = parseInt(process.env.SPOF_KILL_AT_SEC || '0', 10);
  if (STRATEGY === 'central') {
    const controlledServices = topology.services.map(svc => ({
      id: svc.id,
      metricsUrl: `http://${dockerHostResolver(svc.id)}:${svc.port}/metrics`,
      controlUrl: `http://${dockerHostResolver(svc.id)}:${svc.port}/control`,
    }));
    centralController = new CentralController({ services: controlledServices, pollIntervalMs: 2000 });
    centralController.start();
    console.log('[coordinator] Central controller started');
    if (SPOF_KILL_AT_SEC > 0) {
      // Arm kill timer relative to workload start (warmup already elapsed when k6 begins).
      spofKillTimer = setTimeout(() => {
        if (centralController) {
          centralController.stop();
          centralController = null;
          spofKilledAtMs = Date.now();
          console.log(`[coordinator] SPOF: Central controller killed at T+${SPOF_KILL_AT_SEC}s`);
        }
      }, (WARMUP_SEC + SPOF_KILL_AT_SEC) * 1000);
    }
  }

  const workloadResult = await runK6(topology.entryPoint.url, RPS, DURATION_SEC, WARMUP_SEC, RESULTS_DIR);

  if (spofKillTimer) clearTimeout(spofKillTimer);
  if (centralController) centralController.stop();

  const spofMeta = SPOF_KILL_AT_SEC > 0
    ? { killAtSec: SPOF_KILL_AT_SEC, killedAtMs: spofKilledAtMs }
    : null;

  const serviceMetrics = await collectMetrics(topology.services);
  const result = buildResult(workloadResult, serviceMetrics, experimentStartTime, seed, spofMeta);

  console.log(`[coordinator] seed=${seed} SR=${(result.summary.successRate * 100).toFixed(1)}% P90=${result.summary.p90Ms}ms`);

  const filename = resultFilename(seed);
  writeFileSync(filename, JSON.stringify(result, null, 2));
  console.log(`[coordinator] Saved ${filename}`);
}

console.log(`\n[coordinator] Cell complete: ${SEEDS.length} seeds written to ${RESULTS_DIR}/`);
process.exit(0);
