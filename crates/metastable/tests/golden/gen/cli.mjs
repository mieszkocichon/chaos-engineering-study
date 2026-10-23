// stdout of the JS CLIs for the flag sets used by scripts/*.sh and the README (byte-exact target for Rust).
import { writeFileSync } from 'fs';
import { execFileSync } from 'child_process';

const M = 'old-implementation/metastable-failures/src/';
const sp = (s) => s.split(' ');
const P = sp('--baseline 350 --surge-dur 10000 --timeout 300 --attempts 30 --backoff 30 --qmax 2000');
const C = sp('--baseline 350 --surge-dur 8000 --timeout 300 --attempts 30 --backoff 30 --seed 1');
const SW = sp('--lmin 50 --lmax 800 --steps 16 --dwell 4000 --timeout 300 --backoff 30 --qmax 500 --seed 1');
const cases = [
  ['run-m1', []],
  ['run-m1', sp('--baseline 300 --surge 1500 --surge-dur 10000')],
  ['run-m1', [...P, '--surge', '2000', '--seed', '1', '--json']],
  ['run-m1', sp('--cancel-queued --surge 900')],
  ['run-m1', sp('--cancel-queued true --tick 250')],
  ...[600, 800, 1000, 1200, 1500, 2000].map((s) => ['run-m1', [...P, '--surge', String(s), '--seed', '1']]),
  ...[2, 3, 4, 5].map((seed) => ['run-m1', [...P, '--surge', '2000', '--seed', String(seed)]]),
  ...[150, 800].flatMap((q) => [500, 1100, 1800].map((s) => ['run-m1', [...C, '--qmax', String(q), '--surge', String(s)]])),
  ['run-m2', []],
  ['run-m2', sp('--signal queueLen --window 25')],
  ['run-m2', sp('--signal inService')],
  ['run-m2', sp('--signal goodput --window 10')],
  ['run-m2', sp('--l1 400')],
  ['sweep-hysteresis', []],
  ['sweep-hysteresis', sp('--attempts 3')],
  ['sweep-hysteresis', sp('--attempts 3 --json')],
  ...[2, 3, 5, 8, 12, 20].map((a) => ['sweep-hysteresis', [...SW, '--attempts', String(a)]]),
];
const out = cases.map(([bin, args]) => ({
  bin, args,
  stdout: execFileSync('node', [`${M}${bin}.js`, ...args], { encoding: 'utf8', maxBuffer: 1 << 28 }),
}));
writeFileSync(new URL('../cli.json', import.meta.url), JSON.stringify(out) + '\n');
