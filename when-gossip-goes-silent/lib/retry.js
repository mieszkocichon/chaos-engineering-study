export class RetryPolicy {
  constructor({ maxRetries = 3, baseDelayMs = 200, backoffMultiplier = 2, maxDelayMs = 10000 } = {}) {
    this.maxRetries = maxRetries;
    this.baseDelayMs = baseDelayMs;
    this.backoffMultiplier = backoffMultiplier;
    this.maxDelayMs = maxDelayMs;
    this.totalAttempts = 0;
    this.totalRetries = 0;
  }

  async execute(fn) {
    let lastError;
    for (let attempt = 0; attempt <= this.maxRetries; attempt++) {
      this.totalAttempts++;
      try {
        return await fn(attempt);
      } catch (err) {
        lastError = err;
        // CB open means we should stop immediately, not amplify load with retries
        if (err.message && err.message.startsWith('CircuitBreakerOpen')) {
          throw err;
        }
        if (attempt < this.maxRetries) {
          this.totalRetries++;
          const delay = Math.min(
            this.baseDelayMs * Math.pow(this.backoffMultiplier, attempt),
            this.maxDelayMs
          );
          await new Promise(resolve => setTimeout(resolve, delay));
        }
      }
    }
    throw lastError;
  }

  updateParams({ maxRetries, baseDelayMs, backoffMultiplier, maxDelayMs }) {
    if (maxRetries !== undefined) this.maxRetries = maxRetries;
    if (baseDelayMs !== undefined) this.baseDelayMs = baseDelayMs;
    if (backoffMultiplier !== undefined) this.backoffMultiplier = backoffMultiplier;
    if (maxDelayMs !== undefined) this.maxDelayMs = maxDelayMs;
  }

  getStats() {
    return {
      totalAttempts: this.totalAttempts,
      totalRetries: this.totalRetries,
      currentParams: {
        maxRetries: this.maxRetries,
        baseDelayMs: this.baseDelayMs,
        backoffMultiplier: this.backoffMultiplier,
      },
    };
  }

  reset() {
    this.totalAttempts = 0;
    this.totalRetries = 0;
  }
}
