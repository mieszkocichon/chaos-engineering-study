import { writeFileSync } from 'fs';
import { mulberry32 } from '../../../../../old-implementation/metastable-failures/src/sim.js';
import { bits } from './_bits.mjs';

const out = [0, 1, 42, 0xFFFFFFFF, 2 ** 31, 12345.678].map((seed) => {
  const r = mulberry32(seed);
  return { seed, values: Array.from({ length: 10000 }, () => bits(r())) };
});
writeFileSync(new URL('../mulberry32.json', import.meta.url), JSON.stringify(out) + '\n');
