// T038: decision logic of the strategies, driven by scripted observations and a fake clock.
import { writeFileSync } from 'fs';
import { MASAgent } from '../../../../../old-implementation/when-gossip-goes-silent/lib/strategies/mas-agent.js';
import { MASSwimStrategy } from '../../../../../old-implementation/when-gossip-goes-silent/lib/strategies/mas-agent-swim.js';
import { MASInboundStrategy } from '../../../../../old-implementation/when-gossip-goes-silent/lib/strategies/mas-agent-inbound.js';
import { CentralController } from '../../../../../old-implementation/when-gossip-goes-silent/lib/strategies/central.js';
import { BackpressureAgent } from '../../../../../old-implementation/when-gossip-goes-silent/agent/backpressure-agent.js';
import { createGossipReceiver } from '../../../../../old-implementation/when-gossip-goes-silent/lib/gossip.js';
import { mulberry32 } from '../../../../../old-implementation/metastable-failures/src/sim.js';

let clock = 1_700_000_000_000;
Date.now = () => clock;
const pick = (rnd, xs) => xs[Math.floor(rnd() * xs.length)];
const snap = (rnd) => ({
  errorRate: pick(rnd, [0, 0, 0.05, 0.2, 0.6, 1]),
  avgLatency: Math.floor(rnd() * 4000),
  p50: 0, p90: pick(rnd, [100, 400, 499, 500, 1500, 2999, 3000, 3001, 3200, 3500, 5000]),
  p99: 0, requestCount: pick(rnd, [0, 3, 5, 6, 40]), queueDepth: Math.floor(rnd() * 30), windowMs: 10000,
});
const neighbors = ['svc-01', 'svc-02', 'svc-03'].map((id) => ({ id, gossipUrl: `http://${id}:4000/gossip` }));

// ---- MASAgent (_observe/_decide/_act via _tick) ----
const mas = [];
for (let s = 1; s <= 8; s++) {
  const rnd = mulberry32(s);
  clock = 1_700_000_000_000;
  let current = snap(rnd);
  const agent = new MASAgent({ serviceId: 'gateway', metricsCollector: { getSnapshot: () => current }, neighbors });
  const steps = [];
  for (let i = 0; i < 200; i++) {
    clock += pick(rnd, [500, 2000, 2000, 4000, 9000, 12000]);
    const step = { t: clock, gossip: [], breakers: {} };
    for (const n of neighbors) {
      if (rnd() < 0.4) {
        const g = { errorRate: pick(rnd, [0, 0.1, 0.3, 0.31, 0.9]), avgLatency: 10, load: 1, timestamp: rnd() < 0.2 ? 0 : clock - Math.floor(rnd() * 12000) };
        agent.receiveGossip(n.id, g);
        step.gossip.push({ from: n.id, ...g });
      }
    }
    for (const ds of ['svc-01', 'svc-02', 'svc-03', 'svc-04']) {
      if (rnd() < 0.7) {
        const { breaker } = agent._getOrCreate(ds);
        breaker.state = rnd() < 0.3 ? 'open' : 'closed';
        step.breakers[ds] = breaker.state;
      }
    }
    current = snap(rnd);
    step.local = current;
    agent._tick();
    step.params = { ...agent.params };
    step.history = agent.decisionHistory.at(-1);
    step.breakerParams = Object.fromEntries([...agent.breakers].map(([id, b]) => [id, [b.failureThreshold, b.openTimeoutMs]]));
    steps.push(step);
  }
  mas.push({ steps });
}

// ---- CentralController._decide ----
const central = [];
{
  const rnd = mulberry32(77);
  const ids = ['gateway', 'svc-01', 'svc-02', 'svc-03'];
  const cc = new CentralController({ services: ids.map((id) => ({ id })) });
  for (let i = 0; i < 200; i++) {
    clock += 2000;
    const snapshots = new Map(ids.map((id) => [id, rnd() < 0.1 ? { errorRate: 1, avgLatency: 99999, unreachable: true } : snap(rnd)]));
    const id = pick(rnd, ids);
    const decision = cc._decide(id, snapshots);
    central.push({ t: clock, id, snapshots: Object.fromEntries(snapshots), decision: { ...decision }, history: cc.decisionHistory.at(-1) });
  }
}

// ---- BackpressureAgent ----
const backpressure = [];
{
  const rnd = mulberry32(55);
  clock = 1_700_000_000_000;
  const bp = new BackpressureAgent({ serviceId: 'gateway', neighbors });
  for (let i = 0; i < 400; i++) {
    clock += pick(rnd, [0, 100, 900, 2000, 5001, 6000, 9000]);
    const ds = pick(rnd, ['svc-01', 'svc-02', 'svc-03']);
    const op = rnd();
    const step = { t: clock, ds };
    if (op < 0.25) {
      step.op = 'gossip';
      step.data = { errorRate: pick(rnd, [0, 0.29, 0.3, 0.31, 0.75]), load: pick(rnd, [0, 15, 16]), avgLatency: pick(rnd, [10, 2000, 2001]) };
      if (rnd() < 0.8) step.data.timestamp = clock - Math.floor(rnd() * 7000);
      bp.receiveGossip(ds, step.data);
    } else if (op < 0.55) { step.op = 'fail'; bp._recordLocalFailure(ds); }
    else if (op < 0.7) { step.op = 'success'; bp._recordLocalSuccess(ds); }
    else { step.op = 'check'; step.result = bp._shouldThrottle(ds); }
    backpressure.push(step);
  }
}

// ---- Inbound (MASAgentInbound): counters, reports, quirk Q1 via createGossipReceiver ----
const inbound = [];
{
  const rnd = mulberry32(33);
  clock = 1_700_000_000_000;
  let current = snap(rnd);
  const strat = new MASInboundStrategy({ serviceId: 'svc-02', metricsCollector: null, neighbors });
  const agent = strat.agent;
  agent.metricsCollector = { getSnapshot: () => current, getHealthSummary: () => ({ errorRate: current.errorRate, avgLatency: current.avgLatency, load: current.queueDepth }) };
  let pushed = [];
  agent.gossipClient.pushTo = async (url, payload) => { pushed.push({ url, payload: JSON.parse(JSON.stringify(payload)) }); };
  const receive = createGossipReceiver(agent);
  for (let i = 0; i < 300; i++) {
    clock += pick(rnd, [50, 300, 1000, 3000, 9000, 11000]);
    const op = rnd();
    const step = { t: clock };
    if (op < 0.5) { step.op = 'result'; step.ds = pick(rnd, ['svc-01', 'svc-03']); step.ok = rnd() < 0.5; agent.recordDownstreamResult(step.ds, step.ok); }
    else if (op < 0.7) {
      step.op = 'receive';
      step.data = { fromId: 'svc-01', errorRate: 0.1, avgLatency: 5, load: 2, timestamp: clock, inboundReport: { target: 'svc-02', dropRate: 0.5, samples: 9 } };
      receive(step.data);
    } else if (op < 0.75) {
      step.op = 'receiveDirect'; // direct call (bypasses the receiver) – shows what Q1 drops
      step.data = { errorRate: 0.1, avgLatency: 5, load: 2, timestamp: clock, inboundReport: { target: 'svc-02', dropRate: 0.6123456, samples: 9 } };
      agent.receiveGossip('svc-03', step.data);
    } else {
      step.op = 'push'; current = snap(rnd); step.health = agent.metricsCollector.getHealthSummary();
      pushed = []; await agent._gossipPush(); step.pushed = pushed;
    }
    step.stats = JSON.parse(JSON.stringify(agent.getInboundStats()));
    inbound.push(step);
  }
}

// ---- SWIM: which neighbors get gossip each round ----
const swim = [];
for (const [serviceId, n, fanout] of [['gateway', 5, 3], ['svc-17', 8, 3], ['svc-01', 2, 3], ['svc-42', 10, 1], ['x', 6, 4]]) {
  const nbs = Array.from({ length: n }, (_, i) => ({ id: `n${i}`, gossipUrl: `http://n${i}/gossip` }));
  const strat = new MASSwimStrategy({ serviceId, metricsCollector: null, neighbors: nbs, gossipFanout: fanout });
  strat.agent.metricsCollector = { getHealthSummary: () => ({ errorRate: 0, avgLatency: 0, load: 0 }) };
  const rounds = [];
  for (let i = 0; i < 20; i++) {
    const urls = [];
    strat.agent.gossipClient.pushTo = async (url) => { urls.push(url); };
    await strat.agent._gossipPush();
    rounds.push(urls);
  }
  swim.push({ serviceId, n, fanout, rounds });
}

writeFileSync(new URL('../strategies.json', import.meta.url), JSON.stringify({ mas, central, backpressure, inbound, swim }) + '\n');
