// Spike R6: do Rust's ln/cos/sqrt match V8 bit-for-bit on the inputs the simulator and PRNG feed them?
import { writeFileSync } from 'fs';
import { mulberry32 } from '../../../../../old-implementation/metastable-failures/src/sim.js';
import { bits, sha256 } from './_bits.mjs';

const N = 100000;
const out = [];
for (let seed = 1; seed <= 10; seed++) {
  const rng = mulberry32(seed);
  const ln = [], cos = [], sqrt = [];
  for (let i = 0; i < N; i++) {
    const u = rng();
    const x = 1 - u;
    ln.push(bits(Math.log(x)));
    cos.push(bits(Math.cos(2 * Math.PI * u)));
    sqrt.push(bits(Math.sqrt(-2 * Math.log(x))));
  }
  out.push({
    seed, n: N,
    ln: sha256(ln.join(',')), cos: sha256(cos.join(',')), sqrt: sha256(sqrt.join(',')),
    sample: { ln: ln.slice(0, 50), cos: cos.slice(0, 50), sqrt: sqrt.slice(0, 50) },
  });
}
writeFileSync(new URL('../ln_cos.json', import.meta.url), JSON.stringify(out, null, 1) + '\n');
