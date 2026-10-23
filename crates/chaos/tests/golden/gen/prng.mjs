// T036: seedrandom-based PRNG of the experiment service (when-gossip-goes-silent/lib/prng.js).
import { writeFileSync } from 'fs';
import { createPRNG } from '../../../../../old-implementation/when-gossip-goes-silent/lib/prng.js';
import { bits, sha256 } from '../../../../metastable/tests/golden/gen/_bits.mjs';

// full sequences are hashed; a prefix is kept verbatim to locate the first mismatch
const seq = (n, keep, f) => { const xs = Array.from({ length: n }, f); return { n, sha256: sha256(xs.join(',')), head: xs.slice(0, keep) }; };

const seeds = [
  '42', '1000', '2000', '0', '', 'default', '1000_gateway_delay', '2000_svc-01_delay',
  '20000_svc-50_delay', '42_svc-03_delay', 'zażółć gęślą jaźń', '🙂', 'a'.repeat(300),
  'x', 'seed with spaces', '-1', '3.14159', 'NaN', 'null', '1000_svc-10_delay',
];
const out = seeds.map((seed) => {
  const fresh = () => createPRNG(seed);
  const r = fresh();
  const b = fresh(), u = fresh(), n = fresh(), e = fresh();
  return {
    seed,
    random: seq(10000, 200, () => bits(r.random())),
    bernoulli: Array.from({ length: 1000 }, () => b.bernoulli(0.3)),
    uniform: Array.from({ length: 1000 }, () => u.uniform(10, 100)),
    normal: seq(1000, 100, () => bits(n.normalRandom(50, 10))),
    exponential: seq(1000, 100, () => bits(e.exponential(0.5))),
  };
});
writeFileSync(new URL('../prng.json', import.meta.url), JSON.stringify(out) + '\n');
