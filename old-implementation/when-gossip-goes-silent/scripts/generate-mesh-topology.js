#!/usr/bin/env node
// generate-mesh-topology.js
//
// Generator pomocniczej topologii "mesh-N" dla eksperymentu SWIM-like (taki na szybko).
// Tworzy rozszerzone gossip-edges: każdy serwis ma K losowych peerów (stały seed
// dla reprodukowalności), zamiast pojedynczego następnika w pierścieniu.
//
// Usage:
//   node scripts/generate-mesh-topology.js [N] [K]
//
// Defaults: N=50, K=8 (każdy węzeł ma ~8 gossip-peerów).
//
// Wyjście zapisywane w: config/topologies/mesh-N.json
//
// Uwaga: downstream (gateway→svc-i) zostaje bez zmian, bo po co psuć. Zmieniamy wyłącznie
// "kind: gossip" edges — strategia mas-agent-swim losuje spośród nich K
// peerów per rundę gossip (zamiast pełnego fan-outu).

import { writeFileSync, mkdirSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

const N = parseInt(process.argv[2] || '50', 10);
const K = parseInt(process.argv[3] || '8', 10);
const SEED = parseInt(process.argv[4] || '424242', 10);

if (N < 2 || K < 1 || K >= N) {
  console.error(`Bad parameters: N=${N} K=${K} (require N>=2, 1<=K<N)`);
  process.exit(1);
}

// Prosty deterministyczny PRNG (mulberry32) dla powtarzalnego mesha. Musi być powtarzalnie.
function mulberry32(seed) {
  let t = seed >>> 0;
  return function () {
    t |= 0; t = (t + 0x6D2B79F5) | 0;
    let x = Math.imul(t ^ (t >>> 15), 1 | t);
    x = (x + Math.imul(x ^ (x >>> 7), 61 | x)) ^ x;
    return ((x ^ (x >>> 14)) >>> 0) / 4294967296;
  };
}
const rand = mulberry32(SEED);

function pickKDistinct(pool, k, rand) {
  const copy = [...pool];
  const picked = [];
  for (let i = 0; i < k && copy.length > 0; i++) {
    const idx = Math.floor(rand() * copy.length);
    picked.push(copy.splice(idx, 1)[0]);
  }
  return picked;
}

// ── Budowa topologii ──────────────────────────────────────────────────────────
const services = [{ id: 'gateway', role: 'entry' }];
for (let i = 1; i <= N; i++) {
  const id = `svc-${String(i).padStart(2, '0')}`;
  services.push({ id, role: 'service', port: 4000 + i });
}

const edges = [];

// Downstream (jak w ring-N): gateway → każdy svc
for (let i = 1; i <= N; i++) {
  edges.push({ from: 'gateway', to: `svc-${String(i).padStart(2, '0')}` });
}

// Gossip mesh: każdy svc ma K losowych peer'ów (bez samego siebie).
// Krawędzie są kierunkowe, co pozwala strategii swim wybierać losowy podzbiór push-targets (i tak zresztą miało być).
const svcIds = services.filter(s => s.role === 'service').map(s => s.id);
for (const src of svcIds) {
  const pool = svcIds.filter(x => x !== src);
  const peers = pickKDistinct(pool, K, rand);
  for (const dst of peers) {
    edges.push({ from: src, to: dst, kind: 'gossip' });
  }
}

const topology = {
  name: `mesh-${N}`,
  description: `Mesh gossip topology: N=${N} services, K=${K} random peers each (seed=${SEED}).`,
  services,
  edges,
};

// ── Zapis ─────────────────────────────────────────────────────────────────────
const outDir  = resolve(__dirname, '..', 'config', 'topologies');
mkdirSync(outDir, { recursive: true });
const outFile = resolve(outDir, `mesh-${N}.json`);
writeFileSync(outFile, JSON.stringify(topology, null, 2) + '\n');

console.log(`[generate-mesh] Wrote ${outFile} (N=${N}, K=${K}, seed=${SEED})`);
console.log(`[generate-mesh] services=${services.length} edges=${edges.length} (downstream=${N} gossip=${edges.length - N})`);
