import { CircuitBreaker } from '../circuit-breaker.js';
import { RetryPolicy } from '../retry.js';
import { ConcurrencyLimiter } from '../concurrency-limiter.js';

export class StaticStrategy {
  constructor({
    retryCount = 3,
    retryBaseMs = 200,
    retryBackoff = 2,
    cbThreshold = 5,
    cbTimeoutMs = 10000,
    requestTimeoutMs = 3000,
    maxConcurrent = 50,
  } = {}) {
    this.requestTimeoutMs = requestTimeoutMs;
    this.breakers = new Map();
    this.retriers = new Map();
    this.limiters = new Map();
    this._retryConfig = { retryCount, retryBaseMs, retryBackoff };
    this._cbConfig = { cbThreshold, cbTimeoutMs };
    this._maxConcurrent = maxConcurrent;
  }

  _getOrCreate(downstreamId) {
    if (!this.breakers.has(downstreamId)) {
      this.breakers.set(downstreamId, new CircuitBreaker({
        failureThreshold: this._cbConfig.cbThreshold,
        openTimeoutMs: this._cbConfig.cbTimeoutMs,
        name: downstreamId,
      }));
      this.retriers.set(downstreamId, new RetryPolicy({
        maxRetries: this._retryConfig.retryCount,
        baseDelayMs: this._retryConfig.retryBaseMs,
        backoffMultiplier: this._retryConfig.retryBackoff,
      }));
      this.limiters.set(downstreamId, new ConcurrencyLimiter({
        maxConcurrent: this._maxConcurrent,
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
    const timer = setTimeout(() => controller.abort(), this.requestTimeoutMs);

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
    const stats = { strategy: 'static' };
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
  }
}
