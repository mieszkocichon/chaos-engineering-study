export class NoneStrategy {
  constructor({ timeoutMs = 3000 } = {}) {
    this.timeoutMs = timeoutMs;
  }

  async callDownstreams(downstreams) {
    const results = await Promise.all(
      downstreams.map(ds => this._fetch(ds))
    );
    return results;
  }

  async _fetch(downstream) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeoutMs);

    try {
      const res = await fetch(downstream.url, { signal: controller.signal });
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
    return { strategy: 'none', timeoutMs: this.timeoutMs };
  }

  stop() {}
}
