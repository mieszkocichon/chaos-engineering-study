export class CircuitBreaker {
  constructor({ failureThreshold = 5, openTimeoutMs = 10000, name = '' } = {}) {
    this.failureThreshold = failureThreshold;
    this.openTimeoutMs = openTimeoutMs;
    this.name = name;

    this.state = 'closed';
    this.failureCount = 0;
    this.successCount = 0;
    this.lastFailureTime = null;
    this.lastStateChange = Date.now();
    this.stateHistory = [];
  }

  async call(fn) {
    if (this.state === 'open') {
      if (Date.now() - this.lastStateChange >= this.openTimeoutMs) {
        this._transition('half-open');
      } else {
        throw new Error(`CircuitBreakerOpen:${this.name}`);
      }
    }

    try {
      const result = await fn();
      this._onSuccess();
      return result;
    } catch (err) {
      this._onFailure();
      throw err;
    }
  }

  _onSuccess() {
    this.failureCount = 0;
    this.successCount++;
    if (this.state === 'half-open') {
      this._transition('closed');
    }
  }

  _onFailure() {
    this.failureCount++;
    this.lastFailureTime = Date.now();
    if (this.state === 'half-open' || this.failureCount >= this.failureThreshold) {
      this._transition('open');
    }
  }

  _transition(newState) {
    if (this.state !== newState) {
      this.stateHistory.push({
        from: this.state,
        to: newState,
        timestamp: Date.now(),
        failureCount: this.failureCount,
      });
      this.state = newState;
      this.lastStateChange = Date.now();
      if (newState === 'closed') {
        this.failureCount = 0;
      }
    }
  }

  updateParams({ failureThreshold, openTimeoutMs }) {
    if (failureThreshold !== undefined) this.failureThreshold = failureThreshold;
    if (openTimeoutMs !== undefined) this.openTimeoutMs = openTimeoutMs;
  }

  getState() {
    return this.state;
  }

  getStats() {
    return {
      state: this.state,
      failureCount: this.failureCount,
      successCount: this.successCount,
      lastFailureTime: this.lastFailureTime,
      lastStateChange: this.lastStateChange,
      stateHistory: this.stateHistory,
    };
  }

  reset() {
    this.state = 'closed';
    this.failureCount = 0;
    this.successCount = 0;
    this.lastFailureTime = null;
    this.lastStateChange = Date.now();
    this.stateHistory = [];
  }
}
