// Rozwiązanie "gossip silence": gdy tc netem wstrzykuje packet loss,
// server-side errorRate = 0 (pakiety padają przed aplikacją) i gossip milczy.
// Fix: caller mierzy u siebie fail-rate do każdego downstream i osadza ten
// sygnał w gossipie. Odbiorca może wtedy podbić własny errorRate propagowany
// dalej, nawet jeśli lokalnie wszystko wygląda zdrowo.

import { MASAgent } from './mas-agent.js';
import { createGossipReceiver } from '../gossip.js';

const INBOUND_DROP_THRESHOLD = 0.3;  // ≥30% fail-rate do downstream → raport
const INBOUND_MIN_SAMPLES = 5;       // minimum prób w oknie żeby raport miał sens
const INBOUND_WINDOW_MS = 8000;      // okno obserwacji
const INBOUND_REPORT_TTL_MS = 10000; // jak długo B traktuje raport jako aktualny

class MASAgentInbound extends MASAgent {
  constructor(opts) {
    super(opts);
    // per-downstream rolling counters: { ok, fail, windowStart }
    this._dsCounters = new Map();
    // ostatnio odebrany raport o NAS samych
    this._selfInboundReport = null;
    // statystyki PoC (do analizy)
    this._inboundStats = {
      reportsEmitted: 0,
      reportsReceived: 0,
      selfInboundInflations: 0,
    };
  }

  // Callbacks z warstwy strategii — oznaczają ile requestów do danego downstream
  // zakończyło się sukcesem/porażką. Przyjmujemy z MASInboundStrategy._callOne.
  recordDownstreamResult(dsId, ok) {
    const now = Date.now();
    let c = this._dsCounters.get(dsId);
    if (!c || now - c.windowStart > INBOUND_WINDOW_MS) {
      c = { ok: 0, fail: 0, windowStart: now };
      this._dsCounters.set(dsId, c);
    }
    if (ok) c.ok++; else c.fail++;
  }

  _getDownstreamFailRate(dsId) {
    const c = this._dsCounters.get(dsId);
    if (!c) return { rate: 0, samples: 0 };
    const samples = c.ok + c.fail;
    return { rate: samples === 0 ? 0 : c.fail / samples, samples };
  }

  // Override: odbieramy zwykły gossip + wyciągamy `inboundReport` kierowany do nas.
  receiveGossip(fromId, summary) {
    super.receiveGossip(fromId, summary);
    if (summary.inboundReport && summary.inboundReport.target === this.serviceId) {
      this._selfInboundReport = {
        reporterId: fromId,
        dropRate: summary.inboundReport.dropRate,
        timestamp: Date.now(),
      };
      this._inboundStats.reportsReceived++;
    }
  }

  // Override: do health-summary dopinamy (a) self-inbound inflation errorRate,
  // (b) per-peer inboundReport gdy widzimy wysoką stratę do nich.
  async _gossipPush() {
    if (!this.metricsCollector) return;
    const base = this.metricsCollector.getHealthSummary();

    // (a) Self-inflation: jeśli ktoś donosił o nas niedawno, podbijamy własny errorRate
    let summary = base;
    if (this._selfInboundReport &&
        Date.now() - this._selfInboundReport.timestamp < INBOUND_REPORT_TTL_MS) {
      summary = {
        ...base,
        errorRate: Math.max(base.errorRate ?? 0, this._selfInboundReport.dropRate),
        inboundInflatedBy: this._selfInboundReport.reporterId,
      };
      this._inboundStats.selfInflations = (this._inboundStats.selfInflations || 0) + 1;
      this._inboundStats.selfInboundInflations++;
    }

    // (b) Dla każdego peera: jeśli do niego widzimy wysoką stratę, wysyłamy
    // skierowany inboundReport. Reszcie peerów wysyłamy zwykły summary.
    const pushes = this.neighbors.map(n => {
      const { rate, samples } = this._getDownstreamFailRate(n.id);
      let payload = summary;
      if (samples >= INBOUND_MIN_SAMPLES && rate >= INBOUND_DROP_THRESHOLD) {
        payload = {
          ...summary,
          inboundReport: { target: n.id, dropRate: parseFloat(rate.toFixed(4)), samples },
        };
        this._inboundStats.reportsEmitted++;
      }
      return this.gossipClient.pushTo(n.gossipUrl, payload);
    });
    await Promise.all(pushes);
  }

  getInboundStats() {
    return {
      ...this._inboundStats,
      perDownstream: Object.fromEntries(
        [...this._dsCounters.entries()].map(([id, c]) => [
          id, { ok: c.ok, fail: c.fail, rate: this._getDownstreamFailRate(id).rate }
        ])
      ),
      lastSelfReport: this._selfInboundReport,
    };
  }

  reset() {
    super.reset();
    this._dsCounters.clear();
    this._selfInboundReport = null;
    this._inboundStats = { reportsEmitted: 0, reportsReceived: 0, selfInboundInflations: 0 };
  }
}

export class MASInboundStrategy {
  constructor({ serviceId, metricsCollector, neighbors, decisionIntervalMs, gossipIntervalMs }) {
    this.agent = new MASAgentInbound({
      serviceId, metricsCollector, neighbors, decisionIntervalMs, gossipIntervalMs,
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
      const res = await retrier.execute(() =>
        breaker.call(() => this._fetch(downstream.url))
      );
      this.agent.recordDownstreamResult(downstream.id, true);
      return res;
    } catch (err) {
      this.agent.recordDownstreamResult(downstream.id, false);
      throw err;
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
      strategy: 'mas-agent-inbound',
      ...this.agent.getStats(),
      inbound: this.agent.getInboundStats(),
    };
  }

  reset() { this.agent.reset(); }
}
