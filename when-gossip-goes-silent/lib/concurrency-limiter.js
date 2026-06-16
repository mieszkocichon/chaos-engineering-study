export class ConcurrencyLimiter {
  constructor({ maxConcurrent = 50 } = {}) {
    this.maxConcurrent = maxConcurrent;
    this.current = 0;
    this.totalAcquired = 0;
    this.totalRejected = 0;
  }

  acquire() {
    if (this.current < this.maxConcurrent) {
      this.current++;
      this.totalAcquired++;
      return Promise.resolve();
    }
    this.totalRejected++;
    return Promise.reject(
      new Error(`ConcurrencyLimitExceeded: ${this.current}/${this.maxConcurrent}`)
    );
  }

  release() {
    this.current = Math.max(0, this.current - 1);
  }

  updateParams({ maxConcurrent }) {
    if (maxConcurrent !== undefined) {
      this.maxConcurrent = maxConcurrent;
    }
  }

  getStats() {
    return {
      current: this.current,
      max: this.maxConcurrent,
      totalAcquired: this.totalAcquired,
      totalRejected: this.totalRejected,
      shedRate: this.totalAcquired + this.totalRejected > 0
        ? this.totalRejected / (this.totalAcquired + this.totalRejected)
        : 0,
    };
  }

  reset() {
    this.current = 0;
    this.totalAcquired = 0;
    this.totalRejected = 0;
  }
}
