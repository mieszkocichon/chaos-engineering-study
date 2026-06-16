export class GossipClient {
  constructor({ sourceId }) {
    this.sourceId = sourceId;
  }

  async pushTo(targetUrl, healthSummary) {
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 1000);
      await fetch(targetUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          fromId: this.sourceId,
          ...healthSummary,
          timestamp: Date.now(),
        }),
        signal: controller.signal,
      });
      clearTimeout(timer);
    } catch {
      
    }
  }
}

export function createGossipReceiver(agent) {
  return function gossipMiddleware(data) {
    const { fromId, errorRate, avgLatency, load, timestamp } = data;
    if (fromId) {
      agent.receiveGossip(fromId, { errorRate, avgLatency, load, timestamp });
    }
  };
}
