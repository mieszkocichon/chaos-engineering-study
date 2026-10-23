export class WorkloadDriver {
  constructor({ targetUrl, rps = 10, durationSec = 60, warmupSec = 5 }) {
    this.targetUrl = targetUrl;
    this.rps = rps;
    this.durationSec = durationSec;
    this.warmupSec = warmupSec;
  }

  async run() {
    const warmupResults = [];
    const measureResults = [];


    if (this.warmupSec > 0) {
      await this._sendTraffic(this.warmupSec, warmupResults);
    }

    const startTime = Date.now();
    await this._sendTraffic(this.durationSec, measureResults);
    const wallClockMs = Date.now() - startTime;

    return {
      results: measureResults,
      wallClockMs,
      warmupRequests: warmupResults.length,
    };
  }

  async _sendTraffic(durationSec, results) {
    const totalRequests = this.rps * durationSec;
    const intervalMs = 1000 / this.rps;
    let id = 0;

    return new Promise((resolve) => {
      const interval = setInterval(() => {
        id++;
        const reqId = id;

        if (reqId > totalRequests) {
          clearInterval(interval);
          setTimeout(resolve, 5000);
          return;
        }

        const start = Date.now();
        this._sendRequest(reqId)
          .then(({ status, data }) => {
            results.push({
              id: reqId,
              status,
              latencyMs: Date.now() - start,
              timestamp: start,
              success: status === 200,
            });
          })
          .catch((err) => {
            results.push({
              id: reqId,
              status: err.name === 'AbortError' ? 'TIMEOUT' : 'ERROR',
              latencyMs: Date.now() - start,
              timestamp: start,
              success: false,
              error: err.message,
            });
          });
      }, intervalMs);
    });
  }

  async _sendRequest(reqId) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 15000);

    try {
      const res = await fetch(this.targetUrl, {
        signal: controller.signal,
        headers: { 'X-Request-ID': String(reqId) },
      });
      const data = await res.json();
      return { status: res.status, data };
    } finally {
      clearTimeout(timer);
    }
  }
}
