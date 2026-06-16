const http = require('http');
const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
function getArg(name, defaultVal) {
  const idx = args.indexOf(`--${name}`);
  return idx !== -1 && args[idx + 1] ? args[idx + 1] : defaultVal;
}

const TOTAL_ORDERS = parseInt(getArg('orders', '10'), 10);
const SAVE_LABEL = getArg('save', null);
const GATEWAY = 'http://localhost:3000';

function httpRequest(url, options = {}) {
  return new Promise((resolve) => {
    const parsedUrl = new URL(url);
    const opts = {
      hostname: parsedUrl.hostname,
      port: parsedUrl.port,
      path: parsedUrl.pathname,
      method: options.method || 'GET',
      headers: options.headers || {},
      timeout: 15000,
    };

    const req = http.request(opts, (res) => {
      let body = '';
      res.on('data', chunk => body += chunk);
      res.on('end', () => {
        try {
          resolve({ status: res.statusCode, data: JSON.parse(body) });
        } catch {
          resolve({ status: res.statusCode, data: body });
        }
      });
    });

    req.on('error', (err) => resolve({ status: 'ERROR', data: err.message }));
    req.on('timeout', () => { req.destroy(); resolve({ status: 'TIMEOUT', data: 'Timed out' }); });

    if (options.body) req.write(options.body);
    req.end();
  });
}

async function run() {
  const testStart = Date.now();
  console.log(`\n  Sending ${TOTAL_ORDERS} orders to ${GATEWAY}/api/order...\n`);
  console.log('\u2500'.repeat(70));
  console.log('  #    Time     Status   Detail');
  console.log('\u2500'.repeat(70));

  let successes = 0;
  let failures = 0;
  const orderResults = [];

  for (let i = 1; i <= TOTAL_ORDERS; i++) {
    const item = `item-${i}`;
    const amount = 10 + Math.floor(Math.random() * 90);

    const start = Date.now();
    const result = await httpRequest(`${GATEWAY}/api/order`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ item, amount }),
    });
    const duration = Date.now() - start;

    const ok = result.status === 200;
    if (ok) successes++; else failures++;

    const icon = ok ? '\u2705' : '\u274C';
    const dur = `${(duration / 1000).toFixed(2)}s`.padStart(7);
    const detail = ok
      ? `Order #${result.data.order?.id}, Payment #${result.data.payment?.id}`
      : `${result.data.error || result.data}`;
    console.log(`${icon} ${String(i).padStart(3)}  ${dur}     ${String(result.status).padStart(3)}   ${detail}`);

    orderResults.push({
      order: i,
      item,
      amount,
      status: result.status,
      duration,
      orderId: ok ? result.data.order?.id : null,
      paymentId: ok ? result.data.payment?.id : null,
      error: ok ? null : (result.data.error || null),
      orphanedOrderId: result.data?.orphanedOrderId || null,
    });
  }

  const testDuration = Date.now() - testStart;
  console.log('\u2500'.repeat(70));

  const durations = orderResults.map(r => r.duration).sort((a, b) => a - b);
  const avgMs = durations.reduce((s, d) => s + d, 0) / durations.length;
  const p50 = durations[Math.floor(durations.length * 0.50)] || 0;
  const p90 = durations[Math.floor(durations.length * 0.90)] || 0;

  console.log('\n  Checking actual state of services...\n');

  const ordersRes = await httpRequest(`${GATEWAY}/api/orders`);
  const paymentsRes = await httpRequest(`${GATEWAY}/api/payments`);

  const orders = ordersRes.data;
  const payments = paymentsRes.data;

  const orderCount = orders.total || 0;
  const paymentCount = payments.total || 0;
  const created = orders.orders ? orders.orders.filter(o => o.status === 'created').length : 0;
  const cancelled = orders.orders ? orders.orders.filter(o => o.status === 'cancelled').length : 0;
  const orphaned = Math.max(0, created - paymentCount);
  const duplicates = Math.max(0, paymentCount - created);

  console.log('\u2550'.repeat(55));
  console.log('  SAGA CONSISTENCY SCORECARD');
  console.log('\u2550'.repeat(55));
  console.log(`  Orders Attempted    : ${TOTAL_ORDERS}`);
  console.log(`  Successful Responses: ${successes}`);
  console.log(`  Failed Responses    : ${failures}`);
  console.log('\u2500'.repeat(55));
  console.log(`  DB Orders Created   : ${orderCount} (created: ${created}, cancelled: ${cancelled})`);
  console.log(`  DB Payments Processed: ${paymentCount}`);
  console.log('\u2500'.repeat(55));
  console.log(`  Latency Avg         : ${(avgMs / 1000).toFixed(2)}s`);
  console.log(`  Latency P50         : ${(p50 / 1000).toFixed(2)}s`);
  console.log(`  Latency P90         : ${(p90 / 1000).toFixed(2)}s`);
  console.log(`  Total Wall Clock    : ${(testDuration / 1000).toFixed(2)}s`);
  console.log('\u2500'.repeat(55));

  if (created > paymentCount) {
    console.log(`  \u274C INCONSISTENCY: ${orphaned} ORPHANED ORDERS`);
    console.log(`     (Active orders exist without matching payment)`);
  } else if (paymentCount > created) {
    console.log(`  \u274C INCONSISTENCY: ${duplicates} DOUBLE CHARGES`);
    console.log(`     (More payments than active orders - duplicates!)`);
  } else if (created > successes) {
    console.log(`  \u274C INCONSISTENCY: ${created - successes} PHANTOM ORDERS`);
    console.log(`     (More active orders in DB than successful responses - retries created extras)`);
  } else {
    console.log(`  \u2705 SYSTEM CONSISTENT`);
    console.log(`     Orders match Payments perfectly.`);
  }
  console.log('\u2550'.repeat(55));

  if (SAVE_LABEL) {
    const report = {
      label: SAVE_LABEL,
      scenario: 'saga',
      timestamp: new Date().toISOString(),
      config: { totalOrders: TOTAL_ORDERS, gateway: GATEWAY },
      summary: {
        successRate: successes / TOTAL_ORDERS,
        successCount: successes,
        failureCount: failures,
        avgMs: Math.round(avgMs),
        p50,
        p90,
        wallClockMs: testDuration,
        ordersCreated: orderCount,
        ordersActive: created,
        ordersCancelled: cancelled,
        paymentsProcessed: paymentCount,
        orphaned,
        duplicates,
        consistent: orphaned === 0 && duplicates === 0 && created <= successes,
      },
      results: orderResults,
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
