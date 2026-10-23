import { writeFileSync } from 'fs';
import { runTrial, surgeSchedule, nominalCapacityRps } from '../../../../../old-implementation/metastable-failures/src/sim.js';
import { bits, rowsDigest, ROW_FIELDS } from './_bits.mjs';

const configs = {
  default: { surge: { baseline: 300, surge: 1500, surgeStartMs: 15000, surgeDurMs: 10000 }, cfg: {} },
  gateA: {
    surge: { baseline: 350, surge: 2000, surgeStartMs: 15000, surgeDurMs: 10000 },
    cfg: { attemptTimeoutMs: 300, maxAttempts: 30, retryBackoffMs: 30, queueMax: 2000 },
  },
  cancel: {
    surge: { baseline: 350, surge: 1200, surgeStartMs: 10000, surgeDurMs: 8000 },
    cfg: { cancelQueuedOnTimeout: true, totalMs: 40000, tickMs: 250 },
  },
};
const trials = [];
for (const [name, c] of Object.entries(configs)) {
  for (let seed = 1; seed <= 30; seed++) {
    const { rows } = runTrial({ ...c.cfg, seed, lambda: surgeSchedule(c.surge) });
    const entry = { name, seed, nRows: rows.length, digest: rowsDigest(rows) };
    if (seed === 1) entry.rows = rows.map((r) => ROW_FIELDS.map((f) => bits(r[f])));
    trials.push(entry);
  }
}
const nominal = [{}, { capacity: 7, serviceMeanMs: 33 }].map((c) => ({
  cfg: c, bits: bits(nominalCapacityRps({ capacity: 50, serviceMeanMs: 100, ...c })),
}));
writeFileSync(new URL('../sim.json', import.meta.url), JSON.stringify({ configs, trials, nominal }) + '\n');
