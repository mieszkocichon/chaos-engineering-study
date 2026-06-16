import { GossipClient } from './gossip.js';

const LEAF_GOSSIP_INTERVAL_MS = 1000;

export class LeafGossipPusher {
  constructor({ serviceId, metricsCollector, upstreamNeighbors = [] }) {
    this.serviceId = serviceId;
    this.metricsCollector = metricsCollector;
    this.upstreamNeighbors = upstreamNeighbors;
    this.gossipClient = new GossipClient({ sourceId: serviceId });
    this._timer = null;
  }

  start() {
    this._timer = setInterval(() => this._push(), LEAF_GOSSIP_INTERVAL_MS);
  }

  stop() {
    if (this._timer) { clearInterval(this._timer); this._timer = null; }
  }

  async _push() {
    if (!this.metricsCollector) return;
    const summary = this.metricsCollector.getHealthSummary();
    await Promise.allSettled(
      this.upstreamNeighbors.map(n => this.gossipClient.pushTo(n.gossipUrl, summary))
    );
  }
}
