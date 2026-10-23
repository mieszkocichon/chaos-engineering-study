const http = require('http');
const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
function getArg(name, defaultVal) {
  const idx = args.indexOf(`--${name}`);
  return idx !== -1 && args[idx + 1] ? args[idx + 1] : defaultVal;
}

const TOTAL_REQUESTS = parseInt(getArg('requests', '20'), 10);
const SAVE_LABEL = getArg('save', null);
const URL = 'http://localhost:3000/api/combine';

console.log(`\n  Sending ${TOTAL_REQUESTS} concurrent requests to ${URL}...\n`);

function makeRequest(id) {
  return new Promise((resolve) => {
    const start = Date.now();

    const req = http.get(URL, (res) => {
      let body = '';
      res.on('data', chunk => body += chunk);
      res.on('end', () => {
        const duration = Date.now() - start;
        resolve({ id, status: res.statusCode, duration, body });
      });
    });

    req.on('error', (err) => {
      const duration = Date.now() - start;
      resolve({ id, status: 'ERROR', duration, body: err.message });
    });

    req.setTimeout(30000, () => {
      req.destroy();
      const duration = Date.now() - start;
      resolve({ id, status: 'TIMEOUT', duration, body: 'Request timed out (30s)' });
    });
  });
}

function printHistogram(results) {
  const buckets = [
    { label: '< 200ms', max: 200, count: 0 },
    { label: '200ms-1s', max: 1000, count: 0 },
    { label: '1s-5s  ', max: 5000, count: 0 },
    { label: '5s-10s ', max: 10000, count: 0 },
    { label: '> 10s  ', max: Infinity, count: 0 },
  ];

  results.forEach(r => {
    for (const b of buckets) {
      if (r.duration < b.max) { b.count++; break; }
    }
  });

  const maxCount = Math.max(...buckets.map(b => b.count)) || 1;
  console.log('\n  Response Time Histogram:');
  buckets.forEach(b => {
    const bar = '\u2588'.repeat(Math.ceil((b.count / maxCount) * 25));
    if (b.count > 0) console.log(`    ${b.label.padEnd(9)} : ${bar} ${b.count}`);
  });
}

function printTimeline(results) {
  const blocks = ['\u2581', '\u2582', '\u2583', '\u2584', '\u2585', '\u2586', '\u2587', '\u2588'];
  const maxDur = Math.max(...results.map(r => r.duration)) || 1;

  const timeline = results.map(r => {
    const idx = Math.min(Math.floor((r.duration / maxDur) * 7), 7);
    const ok = r.status === 200;
    return ok ? blocks[idx] : '\x1b[31m' + blocks[idx] + '\x1b[0m';
  }).join('');

  console.log(`\n  Latency Timeline (left=first, height=latency):`);
  console.log(`    ${timeline}`);
  console.log(`    ${'0'.padEnd(Math.floor(results.length / 2))}${((maxDur / 1000).toFixed(1) + 's')}`);
}

async function run() {
  const startTime = Date.now();
  const promises = [];
  for (let i = 1; i <= TOTAL_REQUESTS; i++) {
    promises.push(makeRequest(i));
  }

  const results = await Promise.all(promises);
  const totalDuration = Date.now() - startTime;

  console.log('\u2500'.repeat(60));
  console.log('  #   Status   Duration   Result');
  console.log('\u2500'.repeat(60));

  let successes = 0;
  let failures = 0;

  results.forEach(r => {
    const ok = r.status === 200;
    if (ok) successes++; else failures++;

    const icon = ok ? '\u2705' : '\u274C';
    const dur = `${(r.duration / 1000).toFixed(1)}s`.padStart(6);
    const status = String(r.status).padStart(6);
    console.log(`${icon} ${String(r.id).padStart(3)}   ${status}   ${dur}     ${ok ? 'OK' : r.body.substring(0, 50)}`);
  });

  const durations = results.map(r => r.duration).sort((a, b) => a - b);
  const p50 = durations[Math.floor(durations.length * 0.50)] || 0;
  const p90 = durations[Math.floor(durations.length * 0.90)] || 0;
  const p99 = durations[Math.floor(durations.length * 0.99)] || 0;
  const avgMs = durations.reduce((s, d) => s + d, 0) / durations.length;
  const rps = TOTAL_REQUESTS / (totalDuration / 1000);

  console.log('\u2500'.repeat(60));

  printHistogram(results);
  printTimeline(results);

  console.log('\n' + '\u2550'.repeat(50));
  console.log('  LOAD TEST SUMMARY');
  console.log('\u2550'.repeat(50));
  console.log(`  Total Requests : ${TOTAL_REQUESTS}`);
  console.log(`  Wall Clock     : ${(totalDuration / 1000).toFixed(2)}s`);
  console.log('\u2500'.repeat(50));
  console.log(`  Success Rate   : ${successes}/${TOTAL_REQUESTS} (${((successes / TOTAL_REQUESTS) * 100).toFixed(0)}%)`);
  console.log(`  Throughput     : ${rps.toFixed(2)} req/sec`);
  console.log('\u2500'.repeat(50));
  console.log(`  Latency Avg    : ${(avgMs / 1000).toFixed(2)}s`);
  console.log(`  Latency P50    : ${(p50 / 1000).toFixed(2)}s`);
  console.log(`  Latency P90    : ${(p90 / 1000).toFixed(2)}s`);
  console.log(`  Latency P99    : ${(p99 / 1000).toFixed(2)}s`);
  console.log(`  Latency Min    : ${(durations[0] / 1000).toFixed(2)}s`);
  console.log(`  Latency Max    : ${(durations[durations.length - 1] / 1000).toFixed(2)}s`);
  console.log('\u2550'.repeat(50));

  if (SAVE_LABEL) {
    const report = {
      label: SAVE_LABEL,
      scenario: 'circuit-breaker',
      timestamp: new Date().toISOString(),
      config: { totalRequests: TOTAL_REQUESTS, url: URL },
      summary: {
        successRate: successes / TOTAL_REQUESTS,
        successCount: successes,
        failureCount: failures,
        rps: parseFloat(rps.toFixed(2)),
        avgMs: Math.round(avgMs),
        p50,
        p90,
        p99,
        minMs: durations[0],
        maxMs: durations[durations.length - 1],
        wallClockMs: totalDuration,
      },
      results: results.map(r => ({
        id: r.id,
        status: r.status,
        duration: r.duration,
      })),
    };

    const filePath = path.join(__dirname, '..', 'results', `${SAVE_LABEL}.json`);
    fs.writeFileSync(filePath, JSON.stringify(report, null, 2));
    console.log(`\n  Results saved to: results/${SAVE_LABEL}.json`);
  } else {
    console.log(`\n  Tip: use --save <label> to save results for comparison`);
  }
  console.log();
}

run();
