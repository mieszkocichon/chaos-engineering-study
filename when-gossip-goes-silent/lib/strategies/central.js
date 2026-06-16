import { CircuitBreaker } from '../circuit-breaker.js';
import { RetryPolicy } from '../retry.js';
import { ConcurrencyLimiter } from '../concurrency-limiter.js';

const BOUNDS = {
  retryCount: { min: 0, max: 5, default: 3 },
  backoffMultiplier: { min: 1.0, max: 4.0, default: 2.0 },
  cbThreshold: { min: 2, max: 10, default: 5 },
  cbTimeoutMs: { min: 2000, max: 30000, default: 10000 },
  concurrencyLimit: { min: 5, max: 100, default: 50 },
  timeoutMs: { min: 1000, max: 10000, default: 3000 },
};


export class CentralController {
  constructor({ services, pollIntervalMs = 2000 }) {
    this.services = services;
    this.pollIntervalMs = pollIntervalMs;
    this.globalState = new Map();
    this.decisionHistory = [];
    this._timer = null;
    this._perServiceParams = new Map();
  }

  start() {
    this._timer = setInterval(() => this._tick(), this.pollIntervalMs);
  }

  stop() {
    if (this._timer) clearInterval(this._timer);
    this._timer = null;
  }

  async _tick() {
    const snapshots = new Map();
    const polls = this.services.map(async (svc) => {
      try {
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 1000);
        const res = await fetch(svc.metricsUrl, { signal: controller.signal });
        clearTimeout(timer);
        const data = await res.json();
        snapshots.set(svc.id, data.snapshot || data.healthSummary || {});
      } catch {
        snapshots.set(svc.id, { errorRate: 1, avgLatency: 99999, unreachable: true });
      }
    });
    await Promise.all(polls);

    this.globalState = snapshots;

    for (const svc of this.services) {
      const params = this._decide(svc.id, snapshots);
      this._pushControl(svc.controlUrl, params);
    }
  }

  _decide(serviceId, allSnapshots) {
    const local = allSnapshots.get(serviceId) || {};

    if (!this._perServiceParams.has(serviceId)) {
      const init = {};
      for (const [key, b] of Object.entries(BOUNDS)) init[key] = b.default;
      this._perServiceParams.set(serviceId, init);
    }
    const p = { ...this._perServiceParams.get(serviceId) };


    if (local.errorRate > 0.5) {
      p.retryCount = Math.max(BOUNDS.retryCount.min, p.retryCount - 1);
      p.cbThreshold = Math.max(BOUNDS.cbThreshold.min, p.cbThreshold - 1);
      p.concurrencyLimit = Math.max(BOUNDS.concurrencyLimit.min, Math.floor(p.concurrencyLimit * 0.7));
    } else if (local.errorRate < 0.1) {
      p.retryCount = Math.min(BOUNDS.retryCount.max, p.retryCount + 1);
      p.cbThreshold = Math.min(BOUNDS.cbThreshold.max, p.cbThreshold + 1);
      p.concurrencyLimit = Math.min(BOUNDS.concurrencyLimit.max, Math.floor(p.concurrencyLimit * 1.2));
    }

    if (local.p90 > 3000) {
      p.timeoutMs = Math.max(BOUNDS.timeoutMs.min, p.timeoutMs - 500);
      p.backoffMultiplier = Math.min(BOUNDS.backoffMultiplier.max, p.backoffMultiplier + 0.5);
    } else if (local.p90 < 500) {
      p.timeoutMs = Math.min(BOUNDS.timeoutMs.max, p.timeoutMs + 500);
      p.backoffMultiplier = Math.max(BOUNDS.backoffMultiplier.min, p.backoffMultiplier - 0.5);
    }


    for (const [id, snap] of allSnapshots) {
      if (id !== serviceId && snap.errorRate > 0.3) {
        p.concurrencyLimit = Math.max(BOUNDS.concurrencyLimit.min, Math.floor(p.concurrencyLimit * 0.8));
        p.retryCount = Math.max(BOUNDS.retryCount.min, p.retryCount - 1);
        break;
      }
    }


    for (const [key, bounds] of Object.entries(BOUNDS)) {
      p[key] = Math.max(bounds.min, Math.min(bounds.max, p[key]));
    }

    this._perServiceParams.set(serviceId, p);

    this.decisionHistory.push({
      timestamp: Date.now(),
      serviceId,
      decision: { ...p },
    });

    return p;
  }

  async _pushControl(controlUrl, params) {
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 1000);
      await fetch(controlUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(params),
        signal: controller.signal,
      });
      clearTimeout(timer);
    } catch {
      // fire-and-forget
    }
  }

  getStats() {
    return {
      controller: 'central',
      servicesMonitored: this.services.length,
      decisionCount: this.decisionHistory.length,
      history: this.decisionHistory,
    };
  }
}


export class CentralStrategy {
  constructor({ requestTimeoutMs = 3000 } = {}) {
    this.requestTimeoutMs = requestTimeoutMs;
    this.breakers = new Map();
    this.retriers = new Map();
    this.limiters = new Map();
    this.currentParams = Object.fromEntries(Object.entries(BOUNDS).map(([k, b]) => [k, b.default]));
  }

  applyUpdate(params) {
    this.currentParams = { ...this.currentParams, ...params };

    for (const [id, breaker] of this.breakers) {
      breaker.updateParams({
        failureThreshold: this.currentParams.cbThreshold,
        openTimeoutMs: this.currentParams.cbTimeoutMs,
      });
    }
    for (const [id, retrier] of this.retriers) {
      retrier.updateParams({
        maxRetries: this.currentParams.retryCount,
        backoffMultiplier: this.currentParams.backoffMultiplier,
      });
    }
    for (const [id, limiter] of this.limiters) {
      limiter.updateParams({
        maxConcurrent: this.currentParams.concurrencyLimit,
      });
    }
  }

  _getOrCreate(downstreamId) {
    if (!this.breakers.has(downstreamId)) {
      this.breakers.set(downstreamId, new CircuitBreaker({
        failureThreshold: this.currentParams.cbThreshold,
        openTimeoutMs: this.currentParams.cbTimeoutMs,
        name: downstreamId,
      }));
      this.retriers.set(downstreamId, new RetryPolicy({
        maxRetries: this.currentParams.retryCount,
        baseDelayMs: 200,
        backoffMultiplier: this.currentParams.backoffMultiplier,
      }));
      this.limiters.set(downstreamId, new ConcurrencyLimiter({
        maxConcurrent: this.currentParams.concurrencyLimit,
      }));
    }
    return {
      breaker: this.breakers.get(downstreamId),
      retrier: this.retriers.get(downstreamId),
      limiter: this.limiters.get(downstreamId),
    };
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
    const { breaker, retrier, limiter } = this._getOrCreate(downstream.id);

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
    const timer = setTimeout(() => controller.abort(), this.currentParams.timeoutMs);

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
    const stats = { strategy: 'central', currentParams: { ...this.currentParams } };
    for (const [id, breaker] of this.breakers) {
      stats[id] = {
        circuitBreaker: breaker.getStats(),
        retry: this.retriers.get(id).getStats(),
        concurrency: this.limiters.get(id).getStats(),
      };
    }
    return stats;
  }

  stop() {}

  reset() {
    this.breakers.clear();
    this.retriers.clear();
    this.limiters.clear();
    this.currentParams = Object.fromEntries(Object.entries(BOUNDS).map(([k, b]) => [k, b.default]));
  }
}
