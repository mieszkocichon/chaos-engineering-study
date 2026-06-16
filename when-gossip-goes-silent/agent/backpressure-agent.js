import { GossipClient, createGossipReceiver } from '../lib/gossip.js';

const THROTTLE = {
  errorRate: 0.3,
  load: 15,
  avgLatencyMs: 2000,
};

const LOCAL_FAILURE_THRESHOLD = 3;
const LOCAL_FAILURE_WINDOW_MS = 8000;
const LOCAL_RECOVERY_PROBE_MS = 5000;

const GOSSIP_INTERVAL_MS = 1000;
const GOSSIP_STALE_MS = 5000;
const REQUEST_TIMEOUT_MS = 3000;
const MAX_DECISION_LOG = 500;

export class BackpressureAgent {
  constructor({ serviceId, metricsCollector, neighbors = [] }) {
    this.serviceId = serviceId;
    this.metricsCollector = metricsCollector;
    this.neighbors = neighbors;

    this.neighborState = new Map();
    this._localFailures = new Map();
    this.gossipClient = new GossipClient({ sourceId: serviceId });

    this._stats = {
      throttledCount: 0,
      throttledByGossip: 0,
      throttledByLocalFails: 0,
      passedCount: 0,
      gossipReceived: 0,
      throttleLog: [],
    };

    this._gossipTimer = null;
  }

  start() {
    this._gossipTimer = setInterval(() => this._gossipPush(), GOSSIP_INTERVAL_MS);
  }

  stop() {
    if (this._gossipTimer) {
      clearInterval(this._gossipTimer);
      this._gossipTimer = null;
    }
  }

  async _gossipPush() {
    if (!this.metricsCollector) return;
    const summary = this.metricsCollector.getHealthSummary();
    await Promise.allSettled(
      this.neighbors.map(n => this.gossipClient.pushTo(n.gossipUrl, summary))
    );
  }

  receiveGossip(fromId, data) {
    this.neighborState.set(fromId, {
      errorRate: data.errorRate ?? 0,
      avgLatency: data.avgLatency ?? 0,
      load: data.load ?? 0,
      timestamp: data.timestamp ?? Date.now(),
    });
    this._stats.gossipReceived++;
  }

  _shouldThrottle(downstreamId) {
    const state = this.neighborState.get(downstreamId);
    if (state) {
      const age = Date.now() - state.timestamp;
      if (age <= GOSSIP_STALE_MS) {
        const checks = [
          { key: 'errorRate', limit: THROTTLE.errorRate, format: v => `${(v * 100).toFixed(1)}%>${THROTTLE.errorRate * 100}%` },
          { key: 'load', limit: THROTTLE.load, format: v => `${v}>${THROTTLE.load}` },
          { key: 'avgLatency', limit: THROTTLE.avgLatencyMs, format: v => `${v}ms>${THROTTLE.avgLatencyMs}ms` }
        ];

        for (const { key, limit, format } of checks) {
          if (state[key] > limit) {
            return { throttle: true, source: 'gossip', reason: `${key}=${format(state[key])}` };
          }
        }
      }
    }

    const rec = this._localFailures.get(downstreamId);
    if (rec) {
      const now = Date.now();
      if (now - rec.windowStartMs > LOCAL_FAILURE_WINDOW_MS) {
        rec.count = 0;
        rec.windowStartMs = now;
      }
      if (rec.count >= LOCAL_FAILURE_THRESHOLD) {
        // let one probe through periodically to detect recovery
        if (rec.lastThrottledMs && now - rec.lastThrottledMs > LOCAL_RECOVERY_PROBE_MS) {
          rec.lastThrottledMs = null;
          return { throttle: false };
        }
        rec.lastThrottledMs = now;
        return { throttle: true, source: 'local',
          reason: `localFailures=${rec.count}>=${LOCAL_FAILURE_THRESHOLD}` };
      }
    }

    return { throttle: false };
  }

  _recordLocalFailure(downstreamId) {
    const now = Date.now();
    let rec = this._localFailures.get(downstreamId);
    if (!rec) {
      rec = { count: 0, windowStartMs: now, lastThrottledMs: null };
      this._localFailures.set(downstreamId, rec);
    }
    if (now - rec.windowStartMs > LOCAL_FAILURE_WINDOW_MS) {
      rec.count = 0;
      rec.windowStartMs = now;
    }
    rec.count++;
  }

  _recordLocalSuccess(downstreamId) {
    const rec = this._localFailures.get(downstreamId);
    if (rec) {
      rec.count = 0;
      rec.windowStartMs = Date.now();
      rec.lastThrottledMs = null;
    }
  }

  async callDownstreams(downstreams) {
    const results = await Promise.allSettled(
      downstreams.map(ds => this._callOne(ds))
    );
    return results.map((r, i) => {
      if (r.status === 'fulfilled') return r.value;
      return {
        error: r.reason?.message,
        service: downstreams[i].id,
        throttled: r.reason?.throttled ?? false,
        degraded: !r.reason?.throttled,
      };
    });
  }

  async _callOne(downstream) {
    const { throttle, reason, source } = this._shouldThrottle(downstream.id);

    if (throttle) {
      this._stats.throttledCount++;
      if (source === 'gossip') this._stats.throttledByGossip++;
      if (source === 'local') this._stats.throttledByLocalFails++;
      this._stats.throttleLog.push({ ts: Date.now(), downstreamId: downstream.id, reason, source });
      if (this._stats.throttleLog.length > MAX_DECISION_LOG) {
        this._stats.throttleLog.shift();
      }

      const err = new Error(
        `BackpressureThrottled: pre-emptive drop for ${downstream.id} (${reason})`
      );
      err.throttled = true;
      throw err;
    }

    this._stats.passedCount++;
    try {
      const result = await this._fetch(downstream.url);
      this._recordLocalSuccess(downstream.id);
      return result;
    } catch (err) {
      this._recordLocalFailure(downstream.id);
      throw err;
    }
  }

  async _fetch(url) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
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
    const total = this._stats.throttledCount + this._stats.passedCount;
    const neighborStates = Object.fromEntries(
      [...this.neighborState.entries()].map(([id, s]) => [
        id,
        {
          errorRate: parseFloat(s.errorRate.toFixed(4)),
          avgLatency: s.avgLatency,
          load: s.load,
          ageMs: Date.now() - s.timestamp,
        },
      ])
    );

    return {
      strategy: 'backpressure',
      serviceId: this.serviceId,
      throttledCount: this._stats.throttledCount,
      throttledByGossip: this._stats.throttledByGossip,
      throttledByLocalFails: this._stats.throttledByLocalFails,
      passedCount: this._stats.passedCount,
      throttleRate: total > 0 ? parseFloat((this._stats.throttledCount / total).toFixed(4)) : 0,
      gossipReceived: this._stats.gossipReceived,
      neighborStates,
      recentThrottles: this._stats.throttleLog.slice(-20),
    };
  }

  reset() {
    this.neighborState.clear();
    this._localFailures.clear();
    this._stats = {
      throttledCount: 0,
      throttledByGossip: 0,
      throttledByLocalFails: 0,
      passedCount: 0,
      gossipReceived: 0,
      throttleLog: [],
    };
  }
}

export class BackpressureStrategy {
  constructor({ serviceId, metricsCollector, neighbors }) {
    this.agent = new BackpressureAgent({ serviceId, metricsCollector, neighbors });
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
    return this.agent.callDownstreams(downstreams);
  }

  getStats() {
    return this.agent.getStats();
  }

  reset() {
    this.agent.reset();
  }
}
