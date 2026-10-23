// T063: outputs of the JS generators for the argument sets used by the runner scripts.
// Each case runs in a scratch copy of the layout (the generators write next to themselves/cwd).
import { writeFileSync, mkdtempSync, cpSync, readdirSync, readFileSync, statSync, mkdirSync, rmSync } from 'fs';
import { execFileSync } from 'child_process';
import { join } from 'path';
import { tmpdir } from 'os';

const repo = new URL('../../../../../', import.meta.url).pathname;
const W = join(repo, 'old-implementation', 'when-gossip-goes-silent');
const SEEDS20 = Array.from({ length: 20 }, (_, i) => (i + 1) * 1000).join(',');

const cases = [
  { name: 'ring', script: 'ring', args: [] },
  { name: 'ring-swarm-50', script: 'ring-swarm', args: ['50', 'baseline-flaky', 'none', '2000'] },
  { name: 'ring-swarm-defaults', script: 'ring-swarm', args: [] },
  { name: 'ring-swarm-5', script: 'ring-swarm', args: ['5', 'cascade-crash', 'mas-agent', '1000'] },
  { name: 'swarm-20', script: 'swarm', args: ['20', 'baseline-flaky', 'none', '1000'] },
  { name: 'swarm-50-registry', script: 'swarm', args: ['50', 'cascade-crash', 'mas-agent', '2000'], env: { REGISTRY: 'reg.local:5000' } },
  { name: 'swarm-defaults', script: 'swarm', args: [] },
  { name: 'compose-w1', script: 'compose', args: ['ring-50', 'baseline-flaky', 'none', SEEDS20, 'no-wan', '', 'w1'] },
  { name: 'compose-partition', script: 'compose', args: ['ring-50', 'baseline-flaky', 'mas-agent', '1000,2000', 'wan-50ms', 'network-partition', 'w2'] },
  { name: 'compose-env', script: 'compose', args: ['ring-5', 'burst-failure', 'static', '1000', 'no-wan', '', 'sens'],
    env: { STATIC_PARAMS: '{"retryCount":1,"cbThreshold":3}', SWIM_FANOUT: '3', SPOF_KILL_AT_SEC: '30', RESULTS_DIR_HOST: './results-x', RESULTS_DIR_CONTAINER: '/rx', IMAGE: 'custom:1' } },
  { name: 'compose-mesh', script: 'compose', args: ['mesh-50', 'baseline-flaky', 'mas-agent-swim', '1000', 'no-wan', '', 'swim'] },
  { name: 'compose-scale', script: 'compose', args: ['scale-20'] },
  { name: 'compose-defaults', script: 'compose', args: [] },
  { name: 'k8s-20', script: 'k8s', args: ['20', 'baseline-flaky', 'none'] },
  { name: 'k8s-50', script: 'k8s', args: ['50', 'slow-degradation', 'mas-agent'] },
  { name: 'k8s-missing-fault', script: 'k8s', args: ['20', 'nope', 'static'] },
  { name: 'mesh-50', script: 'mesh', args: ['50', '8'] },
  { name: 'mesh-30', script: 'mesh', args: ['30', '8'] },
  { name: 'mesh-10-seed', script: 'mesh', args: ['10', '3', '7'] },
];

function listFiles(dir, base = dir, out = {}) {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) listFiles(p, base, out);
    else out[p.slice(base.length + 1).replaceAll('\\', '/')] = readFileSync(p, 'utf8');
  }
  return out;
}

const out = [];
for (const c of cases) {
  const tmp = mkdtempSync(join(tmpdir(), 'gen-'));
  // layout: <tmp>/w = when-gossip-goes-silent (scripts + config), <tmp>/root = repo root script
  const w = join(tmp, 'w');
  mkdirSync(join(w, 'scripts'), { recursive: true });
  cpSync(join(W, 'config'), join(w, 'config'), { recursive: true });
  for (const f of ['generate-compose-topology.js', 'generate-swarm-topology.js', 'generate-k8s-manifests.js', 'package.json']) cpSync(join(W, f), join(w, f));
  cpSync(join(W, 'scripts', 'generate-mesh-topology.js'), join(w, 'scripts', 'generate-mesh-topology.js'));
  mkdirSync(join(tmp, 'root'));
  cpSync(join(repo, 'old-implementation', 'generate-ring-topology.js'), join(tmp, 'root', 'generate-ring-topology.js'));
  cpSync(join(repo, 'old-implementation', 'generate-swarm-topology.js'), join(tmp, 'root', 'generate-swarm-topology.js'));
  cpSync(join(repo, 'old-implementation', 'package.json'), join(tmp, 'root', 'package.json'));
  const before = listFiles(w);
  const script = { ring: '../root/generate-ring-topology.js', 'ring-swarm': '../root/generate-swarm-topology.js', swarm: 'generate-swarm-topology.js', compose: 'generate-compose-topology.js',
    k8s: 'generate-k8s-manifests.js', mesh: 'scripts/generate-mesh-topology.js' }[c.script];
  const stdout = execFileSync('node', [script, ...c.args], { cwd: w, env: { ...process.env, ...(c.env || {}) }, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  const files = { ...listFiles(w), ...Object.fromEntries(Object.entries(listFiles(join(tmp, 'root'))).map(([k, v]) => [`../root/${k}`, v])) };
  const changed = Object.fromEntries(Object.entries(files).filter(([k, v]) => before[k] !== v && !k.endsWith('.js') && !k.endsWith('package.json')));
  // the legacy root generators print absolute paths; record them repo-relative
  const stdoutRel = stdout.replaceAll(join(tmp, 'root') + '/', '');
  const dirs = readdirSync(w).filter((f) => f.startsWith('results') && statSync(join(w, f)).isDirectory());
  out.push({ ...c, stdout: stdoutRel, files: changed, dirs });
  rmSync(tmp, { recursive: true, force: true });
}
writeFileSync(new URL('../generators.json', import.meta.url), JSON.stringify(out) + '\n');
