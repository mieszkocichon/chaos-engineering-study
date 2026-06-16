const http = require('http');
const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
function getArg(name, defaultVal) {
  const idx = args.indexOf(`--${name}`);
  return idx !== -1 && args[idx + 1] ? args[idx + 1] : defaultVal;
}

const RPS = parseInt(getArg('rps', '2'), 10);
const DURATION = parseInt(getArg('duration', '60'), 10);
const ENDPOINT = getArg('endpoint', '/api/combine');
const METHOD = getArg('method', 'GET').toUpperCase();
const BODY = getArg('body', null);
const SAVE_LABEL = getArg('save', null);
const BASE_URL = 'http://localhost:3000';

const allResults = [];
let running = true;

function makeRequest() {
  return new Promise((resolve) => {
    const start = Date.now();
    const url = new URL(ENDPOINT, BASE_URL);

    const opts = {
      hostname: url.hostname,
      port: url.port,
      path: url.pathname,
      method: METHOD,
      headers: {},
      timeout: 15000,
    };

    if (BODY) {
      opts.headers['Content-Type'] = 'application/json';
    }

    const req = http.request(opts, (res) => {
      let body = '';
      res.on('data', chunk => body += chunk);
      res.on('end', () => {
        const duration = Date.now() - start;
        resolve({ status: res.statusCode, duration, timestamp: Date.now() });
      });
    });

    req.on('error', () => {
      const duration = Date.now() - start;
      resolve({ status: 'ERROR', duration, timestamp: Date.now() });
    });

    req.on('timeout', () => {
      req.destroy();
      const duration = Date.now() - start;
      resolve({ status: 'TIMEOUT', duration, timestamp: Date.now() });
    });

    if (BODY) req.write(BODY);
    req.end();
  });
}

function calcStats(results) {
  if (results.length === 0) return { ok: 0, fail: 0, rate: 0, avg: 0, p50: 0, p90: 0 };
  const ok = results.filter(r => r.status === 200).length;
  const fail = results.length - ok;
  const durations = results.map(r => r.duration).sort((a, b) => a - b);
  const avg = durations.reduce((s, d) => s + d, 0) / durations.length;
  const p50 = durations[Math.floor(durations.length * 0.5)] || 0;
  const p90 = durations[Math.floor(durations.length * 0.9)] || 0;
  return { ok, fail, rate: ok / results.length, avg, p50, p90 };
}

function latencyBar(durationMs, maxMs) {
  const blocks = ['\u2581', '\u2582', '\u2583', '\u2584', '\u2585', '\u2586', '\u2587', '\u2588'];
  const idx = Math.min(Math.floor((durationMs / Math.max(maxMs, 1)) * 7), 7);
  return blocks[idx];
}

function printDashboard(elapsed) {
  const stats = calcStats(allResults);

  const recent = allResults.slice(-40);
  const maxDur = Math.max(...allResults.map(r => r.duration), 1);

  const timeline = recent.map(r => {
    const bar = latencyBar(r.duration, maxDur);
    return r.status === 200 ? bar : '\x1b[31m' + bar + '\x1b[0m';
  }).join('');

  const recentStats = calcStats(allResults.slice(-10));

  if (allResults.length > RPS) {
    process.stdout.write('\x1b[8A\x1b[0J');
  }

  console.log('\u2500'.repeat(60));
  console.log(`  LIVE MONITOR  (${elapsed}s / ${DURATION}s)  ${METHOD} ${ENDPOINT}`);
  console.log('\u2500'.repeat(60));
  console.log(`  Total: ${allResults.length}  OK: ${stats.ok}  Fail: ${stats.fail}  |  Success: ${(stats.rate * 100).toFixed(0)}%`);
  console.log(`  Latency: avg=${(stats.avg / 1000).toFixed(2)}s  p50=${(stats.p50 / 1000).toFixed(2)}s  p90=${(stats.p90 / 1000).toFixed(2)}s`);
  console.log(`  Last 10: avg=${(recentStats.avg / 1000).toFixed(2)}s  success=${(recentStats.rate * 100).toFixed(0)}%`);
  console.log(`  ${timeline}`);
  console.log('\u2500'.repeat(60));
}

function printFinalReport() {
  const stats = calcStats(allResults);
  const durations = allResults.map(r => r.duration).sort((a, b) => a - b);
  const p99 = durations[Math.floor(durations.length * 0.99)] || 0;

  const startTs = allResults[0]?.timestamp || 0;
  const buckets = new Map();
  allResults.forEach(r => {
    const sec = Math.floor((r.timestamp - startTs) / 1000);
    if (!buckets.has(sec)) buckets.set(sec, []);
    buckets.get(sec).push(r);
  });

  console.log('\n' + '\u2550'.repeat(55));
  console.log('  MONITOR FINAL REPORT');
  console.log('\u2550'.repeat(55));
  console.log(`  Duration        : ${DURATION}s`);
  console.log(`  Target RPS      : ${RPS}`);
  console.log(`  Total Requests  : ${allResults.length}`);
  console.log(`  Actual RPS      : ${(allResults.length / DURATION).toFixed(2)}`);
  console.log('\u2500'.repeat(55));
  console.log(`  Success Rate    : ${(stats.rate * 100).toFixed(0)}% (${stats.ok}/${allResults.length})`);
  console.log(`  Latency Avg     : ${(stats.avg / 1000).toFixed(2)}s`);
  console.log(`  Latency P50     : ${(stats.p50 / 1000).toFixed(2)}s`);
  console.log(`  Latency P90     : ${(stats.p90 / 1000).toFixed(2)}s`);
  console.log(`  Latency P99     : ${(p99 / 1000).toFixed(2)}s`);
  console.log(`  Latency Min     : ${(durations[0] / 1000).toFixed(2)}s`);
  console.log(`  Latency Max     : ${(durations[durations.length - 1] / 1000).toFixed(2)}s`);
  console.log('\u2500'.repeat(55));

  console.log('  Per-second success rate:');
  const maxSec = Math.max(...buckets.keys());
  let timelineStr = '    ';
  for (let s = 0; s <= maxSec; s++) {
    const secResults = buckets.get(s) || [];
    if (secResults.length === 0) { timelineStr += '\x1b[90m\u00B7\x1b[0m'; continue; }
    const secRate = secResults.filter(r => r.status === 200).length / secResults.length;
    if (secRate >= 0.8) timelineStr += '\x1b[32m\u2588\x1b[0m';
    else if (secRate >= 0.5) timelineStr += '\x1b[33m\u2584\x1b[0m';
    else if (secRate > 0) timelineStr += '\x1b[31m\u2582\x1b[0m';
    else timelineStr += '\x1b[31m\u2581\x1b[0m';
  }
  console.log(timelineStr);
  console.log('    ' + '\x1b[32m\u2588\x1b[0m=80%+ ' + '\x1b[33m\u2584\x1b[0m=50%+ ' + '\x1b[31m\u2582\x1b[0m=<50% ' + '\x1b[90m\u00B7\x1b[0m=no req');

  console.log('\u2550'.repeat(55));

  if (SAVE_LABEL) {
    const report = {
      label: SAVE_LABEL,
      scenario: 'circuit-breaker',
      timestamp: new Date().toISOString(),
      config: { rps: RPS, duration: DURATION, endpoint: ENDPOINT, method: METHOD },
      summary: {
        successRate: stats.rate,
        successCount: stats.ok,
        failureCount: stats.fail,
        rps: parseFloat((allResults.length / DURATION).toFixed(2)),
        avgMs: Math.round(stats.avg),
        p50: stats.p50,
        p90: stats.p90,
        p99,
        minMs: durations[0],
        maxMs: durations[durations.length - 1],
        wallClockMs: DURATION * 1000,
      },
      results: allResults.map((r, i) => ({
        id: i + 1,
        status: r.status,
        duration: r.duration,
        timestamp: r.timestamp,
      })),
    };

    const filePath = path.join(__dirname, '..', 'results', `${SAVE_LABEL}.json`);
    fs.writeFileSync(filePath, JSON.stringify(report, null, 2));
    console.log(`\n  Results saved to: results/${SAVE_LABEL}.json`);
  }
  console.log();
}

async function run() {
  console.log(`\n  Starting monitor: ${RPS} req/s for ${DURATION}s → ${METHOD} ${BASE_URL}${ENDPOINT}\n`);

  const intervalMs = 1000 / RPS;
  const startTime = Date.now();
  let elapsed = 0;

  const requestInterval = setInterval(async () => {
    if (!running) return;
    const result = await makeRequest();
    allResults.push(result);
  }, intervalMs);

  const dashboardInterval = setInterval(() => {
    elapsed = Math.floor((Date.now() - startTime) / 1000);
    if (allResults.length > 0) {
      printDashboard(elapsed);
    }
  }, 1000);

  setTimeout(() => {
    running = false;
    clearInterval(requestInterval);

    setTimeout(() => {
      clearInterval(dashboardInterval);
      process.stdout.write('\x1b[8A\x1b[0J');
      printFinalReport();
      process.exit(0);
    }, 2000);
  }, DURATION * 1000);

  process.on('SIGINT', () => {
    running = false;
    clearInterval(requestInterval);
    clearInterval(dashboardInterval);
    console.log('\n\n  Monitor stopped by user.\n');
    printFinalReport();
    process.exit(0);
  });
}

run();
