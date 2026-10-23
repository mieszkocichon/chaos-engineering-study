import { writeFileSync } from 'fs';
import { mulberry32 } from '../../../../../old-implementation/metastable-failures/src/sim.js';
import { rollingMean, detrend, rollingVar, rollingAR1, kendallTau, percentile } from '../../../../../old-implementation/metastable-failures/src/ews.js';
import { bits } from './_bits.mjs';

const r = mulberry32(7);
const series = {
  random: Array.from({ length: 120 }, () => r() * 10),
  constant: Array.from({ length: 60 }, () => 3),
  rising: Array.from({ length: 150 }, (_, i) => Math.sin(i) * i * 0.1 + r()),
  short: [1, 2, 3],
  withNulls: [null, null, 1, 2, 3, null, 4, 5, 6, 7, 8, 9, 10, 11, 12],
};
const B = (xs) => xs.map(bits);
const out = [];
for (const [name, xs] of Object.entries(series)) {
  for (const w of [5, 25]) {
    const entry = { name, w, input: B(xs) };
    if (!xs.includes(null)) {
      // rollingMean/detrend only ever see plain numbers in run-m2
      entry.rollingMean = B(rollingMean(xs, w));
      entry.detrend = B(detrend(xs, w));
    }
    entry.rollingVar = B(rollingVar(xs, w));
    entry.rollingAR1 = B(rollingAR1(xs, w));
    entry.kendallTau = bits(kendallTau(xs));
    entry.percentile = [0, 0.5, 0.9, 0.95, 0.99, 1].map((p) => bits(percentile(xs, p)));
    out.push(entry);
  }
}
writeFileSync(new URL('../ews.json', import.meta.url), JSON.stringify(out) + '\n');
