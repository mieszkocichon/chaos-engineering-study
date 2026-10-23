import { CircuitBreaker } from '../circuit-breaker.js';
import { RetryPolicy } from '../retry.js';
import { ConcurrencyLimiter } from '../concurrency-limiter.js';
import { GossipClient, createGossipReceiver } from '../gossip.js';


const BOUNDS = {
  retryCount: { min: 0, max: 5, default: 3 },
  backoffMultiplier: { min: 1.0, max: 4.0, default: 2.0 },
  cbThreshold: { min: 2, max: 10, default: 5 },
  cbTimeoutMs: { min: 2000, max: 30000, default: 10000 },
  concurrencyLimit: { min: 5, max: 100, default: 50 },
  timeoutMs: { min: 1000, max: 10000, default: 3000 },
};

export class MASAgent {
  constructor({
    serviceId,
    metricsCollector,
    neighbors = [],
    decisionIntervalMs = 2000,
    gossipIntervalMs = 3000,
  }) {
    this.serviceId = serviceId;
    this.metricsCollector = metricsCollector;
    this.neighbors = neighbors;
    this.decisionIntervalMs = decisionIntervalMs;
    this.gossipIntervalMs = gossipIntervalMs;


    this.params = this._defaultParams();

    this.neighborState = new Map();
    this.gossipStaleMs = gossipIntervalMs * 3;

    this.gossipClient = new GossipClient({ sourceId: serviceId });

    this.breakers = new Map();
    this.retriers = new Map();
    this.limiters = new Map();

    this.decisionHistory = [];
    this._prevP90 = 0;

    this._decisionTimer = null;
    this._gossipTimer = null;
  }

  _defaultParams() {
    return Object.fromEntries(Object.entries(BOUNDS).map(([k, b]) => [k, b.default]));
  }

  start() {
    this._decisionTimer = setInterval(() => this._tick(), this.decisionIntervalMs);
    this._gossipTimer = setInterval(() => this._gossipPush(), this.gossipIntervalMs);
  }

  stop() {
    if (this._decisionTimer) clearInterval(this._decisionTimer);
    if (this._gossipTimer) clearInterval(this._gossipTimer);
    this._decisionTimer = null;
    this._gossipTimer = null;
  }


  _observe() {
    const local = this.metricsCollector.getSnapshot();

    const openBreakers = [];
    for (const [id, cb] of this.breakers) {
      if (cb.getState() === 'open') openBreakers.push(id);
    }

    const now = Date.now();
    const activeNeighbors = new Map();
    for (const [id, data] of this.neighborState) {
      if (now - data.timestamp < this.gossipStaleMs) {
        activeNeighbors.set(id, data);
      }
    }

    return { local, neighbors: activeNeighbors, openBreakers };
  }


  _decide(observation) {
    const { local, neighbors, openBreakers } = observation;
    const p = { ...this.params };

    const openCount = openBreakers.length;
    const totalTracked = this.breakers.size;

    if (openCount > 0 && totalTracked > 0) {
      const degradedFraction = openCount / totalTracked;

      p.retryCount = Math.max(BOUNDS.retryCount.min, p.retryCount - 1);
      p.cbThreshold = Math.max(BOUNDS.cbThreshold.min, p.cbThreshold - 1);

      p.concurrencyLimit = Math.max(
        BOUNDS.concurrencyLimit.min,
        Math.floor(p.concurrencyLimit * (1 - Math.min(degradedFraction, 0.4)))
      );
    } else if (openCount === 0 && totalTracked > 0 && local.requestCount > 5) {
      p.retryCount = Math.min(BOUNDS.retryCount.max, p.retryCount + 1);
      p.cbThreshold = Math.min(BOUNDS.cbThreshold.max, p.cbThreshold + 1);
      p.concurrencyLimit = Math.min(
        BOUNDS.concurrencyLimit.max,
        Math.floor(p.concurrencyLimit * 1.15)
      );
    }

    // trend matters here — rising p90 with no open breakers means congestion, not failure
    const latencyTrend = local.p90 - this._prevP90;
    this._prevP90 = local.p90;

    if (local.p90 > 3000) {
      if (latencyTrend > 150 && openCount === 0) {
        p.timeoutMs = Math.min(BOUNDS.timeoutMs.max, p.timeoutMs + 1000);
        p.backoffMultiplier = Math.max(BOUNDS.backoffMultiplier.min, p.backoffMultiplier - 0.5);
      } else {
        p.timeoutMs = Math.max(BOUNDS.timeoutMs.min, p.timeoutMs - 500);
        p.backoffMultiplier = Math.min(BOUNDS.backoffMultiplier.max, p.backoffMultiplier + 0.5);
      }
    } else if (local.p90 < 500 && local.requestCount > 5) {
      p.timeoutMs = Math.min(BOUNDS.timeoutMs.max, p.timeoutMs + 500);
      p.backoffMultiplier = Math.max(BOUNDS.backoffMultiplier.min, p.backoffMultiplier - 0.5);
    }

    for (const [nId, nHealth] of neighbors) {
      if (nHealth.errorRate > 0.3) {
        p.concurrencyLimit = Math.max(
          BOUNDS.concurrencyLimit.min,
          Math.floor(p.concurrencyLimit * 0.8)
        );
        p.retryCount = Math.max(BOUNDS.retryCount.min, p.retryCount - 1);
        break; // one bad neighbor is enough
      }
    }

    for (const [key, bounds] of Object.entries(BOUNDS)) {
      p[key] = Math.max(bounds.min, Math.min(bounds.max, p[key]));
    }

    return p;
  }

  _act(newParams) {
    this.params = newParams;

    for (const [id, breaker] of this.breakers) {
      breaker.updateParams({
        failureThreshold: newParams.cbThreshold,
        openTimeoutMs: newParams.cbTimeoutMs,
      });
    }
    for (const [id, retrier] of this.retriers) {
      retrier.updateParams({
        maxRetries: newParams.retryCount,
        backoffMultiplier: newParams.backoffMultiplier,
      });
    }
    for (const [id, limiter] of this.limiters) {
      limiter.updateParams({
        maxConcurrent: newParams.concurrencyLimit,
      });
    }
  }


  _tick() {
    const observation = this._observe();
    const decision = this._decide(observation);

    this.decisionHistory.push({
      timestamp: Date.now(),
      observation: {
        errorRate: observation.local.errorRate,
        p90: observation.local.p90,
        requestCount: observation.local.requestCount,
        queueDepth: observation.local.queueDepth,
        neighborCount: observation.neighbors.size,
        openBreakers: observation.openBreakers,
      },
      decision: { ...decision },
    });

    this._act(decision);
  }


  async _gossipPush() {
    const summary = this.metricsCollector.getHealthSummary();
    const pushes = this.neighbors.map(n =>
      this.gossipClient.pushTo(n.gossipUrl, summary)
    );
    await Promise.all(pushes);
  }

  receiveGossip(fromId, summary) {
    this.neighborState.set(fromId, {
      ...summary,
      timestamp: summary.timestamp || Date.now(),
    });
  }


  _getOrCreate(downstreamId) {
    if (!this.breakers.has(downstreamId)) {
      this.breakers.set(downstreamId, new CircuitBreaker({
        failureThreshold: this.params.cbThreshold,
        openTimeoutMs: this.params.cbTimeoutMs,
        name: `${this.serviceId}->${downstreamId}`,
      }));
      this.retriers.set(downstreamId, new RetryPolicy({
        maxRetries: this.params.retryCount,
        baseDelayMs: 200,
        backoffMultiplier: this.params.backoffMultiplier,
      }));
      this.limiters.set(downstreamId, new ConcurrencyLimiter({
        maxConcurrent: this.params.concurrencyLimit,
      }));
    }
    return {
      breaker: this.breakers.get(downstreamId),
      retrier: this.retriers.get(downstreamId),
      limiter: this.limiters.get(downstreamId),
    };
  }

  getStats() {
    return {
      serviceId: this.serviceId,
      currentParams: { ...this.params },
      decisionCount: this.decisionHistory.length,
      neighborCount: this.neighborState.size,
      history: this.decisionHistory,
    };
  }

  reset() {
    this.neighborState.clear();
    this.breakers.clear();
    this.retriers.clear();
    this.limiters.clear();
    this.decisionHistory = [];
    this._prevP90 = 0;
    this.params = this._defaultParams();
  }
}


export class MASStrategy {
  constructor({ serviceId, metricsCollector, neighbors, decisionIntervalMs, gossipIntervalMs }) {
    this.agent = new MASAgent({
      serviceId,
      metricsCollector,
      neighbors,
      decisionIntervalMs,
      gossipIntervalMs,
    });
  }

  start() {
    this.agent.start();
  }

  stop() {
    this.agent.stop();
  }

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
    return { strategy: 'mas-agent', ...this.agent.getStats() };
  }

  reset() {
    this.agent.reset();
  }
}
