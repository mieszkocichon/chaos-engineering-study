// T039: topology loader (ports, downstreams, neighbors, defaults) and fault profiles.
import { writeFileSync, readdirSync } from 'fs';
import { loadTopology, loadFaultProfile } from '../../../../../old-implementation/when-gossip-goes-silent/lib/topology-loader.js';

const cfg = new URL('../../../../../old-implementation/when-gossip-goes-silent/config/', import.meta.url);
const names = (dir) => readdirSync(new URL(dir, cfg)).filter((f) => f.endsWith('.json')).map((f) => f.slice(0, -5)).sort();

const topologies = [];
for (const name of names('topologies/')) {
  for (const basePort of name.startsWith('mesh') || name.endsWith('50') ? [4000] : [4000, 4040, 3650]) {
    const t = loadTopology(name, { basePort, hostResolver: (id) => id });
    topologies.push({ name, basePort, ...t, portMap: Object.fromEntries(t.portMap) });
  }
}
const faults = names('faults/').map((name) => ({ name, profile: loadFaultProfile(name) }));
writeFileSync(new URL('../topology.json', import.meta.url), JSON.stringify({ topologies, faults }) + '\n');
