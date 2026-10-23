// SWIM-variant: instead of pushing to all neighbors (O(N)),
// each round picks K random peers — O(log N) propagation in mesh topologies.

import { MASAgent } from './mas-agent.js';
import { createGossipReceiver } from '../gossip.js';

const DEFAULT_FANOUT = 3;

class MASAgentSwim extends MASAgent {
  constructor(opts) {
    super(opts);
    this.gossipFanout = Math.max(1, parseInt(opts.gossipFanout ?? DEFAULT_FANOUT, 10));
    this._prng = mulberry32((opts.serviceId ?? 'x').split('').reduce((a, c) => a + c.charCodeAt(0), 0) ^ 0xC0FFEE);
  }

  // Override: losowy subset sąsiadów zamiast pełnego fan-outu.
  async _gossipPush() {
    if (!this.metricsCollector) return;
    const summary = this.metricsCollector.getHealthSummary();
    const picks = pickK(this.neighbors, this.gossipFanout, this._prng);
    const pushes = picks.map(n => this.gossipClient.pushTo(n.gossipUrl, summary));
    await Promise.all(pushes);
  }
}

function mulberry32(seed) {
  let t = seed >>> 0;
  return function () {
    t |= 0; t = (t + 0x6D2B79F5) | 0;
    let x = Math.imul(t ^ (t >>> 15), 1 | t);
    x = (x + Math.imul(x ^ (x >>> 7), 61 | x)) ^ x;
    return ((x ^ (x >>> 14)) >>> 0) / 4294967296;
  };
}

function pickK(arr, k, rand) {
  if (arr.length <= k) return arr;
  const copy = [...arr];
  const out = [];
  for (let i = 0; i < k; i++) {
    const idx = Math.floor(rand() * copy.length);
    out.push(copy.splice(idx, 1)[0]);
  }
  return out;
}

export class MASSwimStrategy {
  constructor({ serviceId, metricsCollector, neighbors, decisionIntervalMs, gossipIntervalMs, gossipFanout }) {
    this.agent = new MASAgentSwim({
      serviceId,
      metricsCollector,
      neighbors,
      decisionIntervalMs,
      gossipIntervalMs,
      gossipFanout,
    });
  }

  start() { this.agent.start(); }
  stop()  { this.agent.stop(); }

  getGossipReceiver() {
    return createGossipReceiver(this.agent);
  }

  async callDownstreams(downstreams) {
    const results = await Promise.allSettled(
      downstreams.map(ds => this._callOne(ds))
    );
    return results.map((r, i) => {
      if (r.status === 'fulfilled') return r.value;
      return { error: r.reason?.message, service: downstreams[i].id, degraded: true };
    });
  }

  async _callOne(downstream) {
    const { breaker, retrier, limiter } = this.agent._getOrCreate(downstream.id);
    await limiter.acquire();
    try {
      return await retrier.execute(() =>
        breaker.call(() => this._fetch(downstream.url))
      );
    } finally {
      limiter.release();
    }
  }

  async _fetch(url) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.agent.params.timeoutMs);
    try {
      const res = await fetch(url, { signal: controller.signal });
      if (!res.ok) {
        const body = await res.text();
        throw new Error(`HTTP ${res.status}: ${body}`);
      }
      return await res.json();
    } finally {
      clearTimeout(timer);
    }
  }

  getStats() {
    return {
      strategy: 'mas-agent-swim',
      gossipFanout: this.agent.gossipFanout,
      ...this.agent.getStats(),
    };
  }

  reset() { this.agent.reset(); }
}
