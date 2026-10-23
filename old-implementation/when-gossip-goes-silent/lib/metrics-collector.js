export class MetricsCollector {
  constructor({ windowSizeMs = 10000, bucketMs = 1000 } = {}) {
    this.windowSizeMs = windowSizeMs;
    this.bucketMs = bucketMs;
    this.bucketCount = Math.ceil(windowSizeMs / bucketMs);
    this.buckets = [];
    this.inFlight = 0;
    this._initBuckets();
  }

  _initBuckets() {
    this.buckets = [];
    for (let i = 0; i < this.bucketCount; i++) {
      this.buckets.push(this._emptyBucket());
    }
  }

  // Soft-reset for seed iteration within a cell: clears sliding-window buckets
  // and in-flight counter without destroying the collector instance.
  reset() {
    this.inFlight = 0;
    this._initBuckets();
  }

  _emptyBucket() {
    return { count: 0, errors: 0, latencies: [], timestamp: 0 };
  }

  _currentBucketIndex() {
    return Math.floor(Date.now() / this.bucketMs) % this.bucketCount;
  }

  _getBucket() {
    const idx = this._currentBucketIndex();
    const now = Math.floor(Date.now() / this.bucketMs);
    const bucket = this.buckets[idx];
    if (bucket.timestamp !== now) {
      this.buckets[idx] = this._emptyBucket();
      this.buckets[idx].timestamp = now;
    }
    return this.buckets[idx];
  }

  startRequest() {
    this.inFlight++;
  }

  endRequest() {
    this.inFlight = Math.max(0, this.inFlight - 1);
  }

  recordRequest({ latencyMs, success }) {
    this.endRequest();
    const bucket = this._getBucket();
    bucket.count++;
    if (!success) bucket.errors++;
    bucket.latencies.push(latencyMs);
  }

  _getActiveBuckets() {
    const now = Math.floor(Date.now() / this.bucketMs);
    return this.buckets.filter(b => b.timestamp > 0 && (now - b.timestamp) < this.bucketCount);
  }

  getSnapshot() {
    const active = this._getActiveBuckets();
    if (active.length === 0) {
      return {
        errorRate: 0, avgLatency: 0, p50: 0, p90: 0, p99: 0,
        requestCount: 0, queueDepth: this.inFlight, windowMs: this.windowSizeMs,
      };
    }

    let totalCount = 0;
    let totalErrors = 0;
    const allLatencies = [];

    for (const b of active) {
      totalCount += b.count;
      totalErrors += b.errors;
      allLatencies.push(...b.latencies);
    }

    allLatencies.sort((a, b) => a - b);

    const errorRate = totalCount > 0 ? totalErrors / totalCount : 0;
    const avgLatency = allLatencies.length > 0
      ? allLatencies.reduce((s, v) => s + v, 0) / allLatencies.length
      : 0;

    return {
      errorRate,
      avgLatency: Math.round(avgLatency),
      p50: this._percentile(allLatencies, 0.5),
      p90: this._percentile(allLatencies, 0.9),
      p99: this._percentile(allLatencies, 0.99),
      requestCount: totalCount,
      queueDepth: this.inFlight,
      windowMs: this.windowSizeMs,
    };
  }

  getHealthSummary() {
    const snap = this.getSnapshot();
    return {
      errorRate: snap.errorRate,
      avgLatency: snap.avgLatency,
      load: snap.queueDepth,
    };
  }

  _percentile(sorted, p) {
    if (sorted.length === 0) return 0;
    const idx = Math.ceil(sorted.length * p) - 1;
    return sorted[Math.max(0, idx)];
  }
}
