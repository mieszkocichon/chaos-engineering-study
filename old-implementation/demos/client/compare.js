const fs = require('fs');
const path = require('path');

const [fileA, fileB] = process.argv.slice(2);

if (!fileA || !fileB) {
  console.log('\n  Usage: node client/compare.js <before.json> <after.json>\n');
  console.log('  Example:');
  console.log('    node client/compare.js results/before-cb.json results/after-cb.json\n');
  process.exit(1);
}

function loadReport(filePath) {
  const resolved = path.resolve(filePath);
  return JSON.parse(fs.readFileSync(resolved, 'utf-8'));
}

function pctChange(before, after) {
  if (before === 0) return after === 0 ? 0 : Infinity;
  return ((after - before) / Math.abs(before)) * 100;
}

function formatPct(pct, higherIsBetter) {
  if (!isFinite(pct)) return '     N/A';
  const sign = pct > 0 ? '+' : '';
  const str = `${sign}${pct.toFixed(0)}%`.padStart(8);
  const good = higherIsBetter ? pct > 0 : pct < 0;
  if (Math.abs(pct) < 1) return '\x1b[90m' + str + '\x1b[0m';
  return good ? '\x1b[32m' + str + ' \u2705\x1b[0m' : '\x1b[31m' + str + ' \u274C\x1b[0m';
}

function formatVal(val, unit) {
  if (unit === '%') return `${(val * 100).toFixed(0)}%`.padStart(10);
  if (unit === 'rps') return `${val.toFixed(2)} rps`.padStart(10);
  if (unit === 's') return `${(val / 1000).toFixed(2)}s`.padStart(10);
  if (unit === 'ms') return `${val}ms`.padStart(10);
  if (unit === 'int') return `${val}`.padStart(10);
  if (unit === 'bool') return (val ? '\u2705 yes' : '\u274C no').padStart(10);
  return String(val).padStart(10);
}

function printRow(label, before, after, unit, higherIsBetter) {
  const bStr = formatVal(before, unit);
  const aStr = formatVal(after, unit);
  const pct = pctChange(before, after);
  const pctStr = formatPct(pct, higherIsBetter);
  console.log(`  ${label.padEnd(22)} \u2502 ${bStr} \u2502 ${aStr} \u2502 ${pctStr}`);
}

function printSeparator() {
  console.log('  ' + '\u2500'.repeat(22) + '\u2500\u253C\u2500' + '\u2500'.repeat(11) + '\u2500\u253C\u2500' + '\u2500'.repeat(11) + '\u2500\u253C\u2500' + '\u2500'.repeat(12));
}

const a = loadReport(fileA);
const b = loadReport(fileB);

console.log('\n' + '\u2550'.repeat(62));
console.log(`  COMPARISON: ${a.label}  vs  ${b.label}`);
console.log('\u2550'.repeat(62));
console.log(`  Scenario  : ${a.scenario}`);
console.log(`  Before    : ${a.timestamp}`);
console.log(`  After     : ${b.timestamp}`);
console.log('\u2550'.repeat(62));

console.log(`  ${'Metric'.padEnd(22)} \u2502 ${'Before'.padStart(10)} \u2502 ${'After'.padStart(10)} \u2502 ${'Change'.padStart(10)}`);
printSeparator();

printRow('Success Rate', a.summary.successRate, b.summary.successRate, '%', true);

if (a.scenario === 'circuit-breaker') {
  printRow('Throughput', a.summary.rps, b.summary.rps, 'rps', true);
  printRow('Latency Avg', a.summary.avgMs, b.summary.avgMs, 's', false);
  printRow('Latency P50', a.summary.p50, b.summary.p50, 's', false);
  printRow('Latency P90', a.summary.p90, b.summary.p90, 's', false);
  printRow('Latency P99', a.summary.p99, b.summary.p99, 's', false);
  printRow('Latency Max', a.summary.maxMs, b.summary.maxMs, 's', false);
} else if (a.scenario === 'saga') {
  printRow('Latency Avg', a.summary.avgMs, b.summary.avgMs, 's', false);
  printRow('Latency P50', a.summary.p50, b.summary.p50, 's', false);
  printRow('Latency P90', a.summary.p90, b.summary.p90, 's', false);
  printSeparator();
  printRow('Orders Created', a.summary.ordersCreated, b.summary.ordersCreated, 'int', false);
  printRow('Payments Processed', a.summary.paymentsProcessed, b.summary.paymentsProcessed, 'int', true);
  printRow('Orphaned Orders', a.summary.orphaned, b.summary.orphaned, 'int', false);
  printRow('Double Charges', a.summary.duplicates, b.summary.duplicates, 'int', false);
}

console.log('\u2550'.repeat(62));

const sA = a.summary;
const sB = b.summary;
let verdict = '';

if (a.scenario === 'circuit-breaker') {
  const latencyImproved = sB.p90 < sA.p90 * 0.5;
  const rpsImproved = sB.rps > sA.rps * 1.5;
  const successImproved = sB.successRate > sA.successRate;

  if (latencyImproved || rpsImproved || successImproved) {
    verdict = '\x1b[32m  VERDICT: System IMPROVED after applying pattern\x1b[0m';
  } else if (sB.p90 > sA.p90 * 1.2) {
    verdict = '\x1b[31m  VERDICT: System DEGRADED\x1b[0m';
  } else {
    verdict = '\x1b[33m  VERDICT: No significant change\x1b[0m';
  }
} else {
  const wasInconsistent = sA.orphaned > 0 || sA.duplicates > 0;
  const nowConsistent = sB.orphaned === 0 && sB.duplicates === 0;

  if (wasInconsistent && nowConsistent) {
    verdict = '\x1b[32m  VERDICT: Data consistency RESTORED! Saga pattern working.\x1b[0m';
  } else if (!wasInconsistent) {
    verdict = '\x1b[33m  VERDICT: Before was already consistent (re-run with more orders?)\x1b[0m';
  } else {
    const improved = (sB.orphaned + sB.duplicates) < (sA.orphaned + sA.duplicates);
    verdict = improved
      ? '\x1b[33m  VERDICT: Improved but still inconsistent\x1b[0m'
      : '\x1b[31m  VERDICT: Still inconsistent — saga not working yet\x1b[0m';
  }
}

console.log(verdict);
console.log('\u2550'.repeat(62));
console.log();
