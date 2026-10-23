import express from 'express';
import fetch from 'node-fetch';

const app = express();
const PORT = process.env.GATEWAY_PORT || 3000;
const MAX_RETRIES = 1;

const services = {
  a: 'http://localhost:3001/data',
  b: 'http://localhost:3002/data',
};

class CircuitBreaker {

  constructor({ failureThreshold = 5, openTimeout = 30000, name = '' }) {
    this.failureThreshold = failureThreshold;
    this.openTimeout = openTimeout;
    this.name = name;

    this.state = 'closed';
    this.failureCnt = 0;
    this.nextAttempt = 0;
  }

  async call(fn) {
    const now = Date.now();

    if (this.state === 'open') {
      if (now < this.nextAttempt) {
        console.log(`[${this.name}] Circuit breaker OPEN – skipping call`);
        throw new Error(`CircuitBreakerOpen: service ${this.name} unavailable`);
      }

      this.state = 'half-open';
      console.log(`[${this.name}] Circuit breaker HALF‑OPEN – retrying once`);
    }

    try {
      const result = await fn();

      this.failureCnt = 0;
      if (this.state !== 'closed') {
        console.log(`[${this.name}] Circuit breaker CLOSED after success`);
      }
      this.state = 'closed';
      return result;

    } catch (err) {

      this.failureCnt += 1;
      console.log(`[${this.name}] Circuit breaker failure #${this.failureCnt}`);

      if (this.state === 'half-open' || this.failureCnt >= this.failureThreshold) {
        this.state = 'open';
        this.nextAttempt = now + this.openTimeout;
        console.log(`[${this.name}] Circuit breaker OPEN – will retry after ${this.openTimeout} ms`);
      }

      throw err;
    }
  }
}

const breakers = {
  a: new CircuitBreaker({ failureThreshold: 5, openTimeout: 10000, name: 'A' }),
  b: new CircuitBreaker({ failureThreshold: 3, openTimeout: 5000, name: 'B' }),
};

async function fetchWithCB(url, breaker, retries = MAX_RETRIES) {
  for (let attempt = 1; attempt <= retries; attempt++) {
    try {
      return await breaker.call(async () => {
        const res = await fetch(url);
        if (!res.ok) {
          throw new Error(`HTTP ${res.status} – ${res.statusText}`);
        }
        return await res.json();
      });
    } catch (err) {

      if (err.message.startsWith('CircuitBreakerOpen')) {
        throw err;
      }
      console.log(`  RETRY ${attempt}/${retries} for ${url} → ${err.message}`);
      if (attempt === retries) throw err;
    }
  }
}

app.get('/api/combine', async (req, res, next) => {
  console.log('\n--- New request to /api/combine ---');
  try {
    const [resultA, resultB] = await Promise.allSettled([
      fetchWithCB(services.a, breakers.a),
      fetchWithCB(services.b, breakers.b),
    ]);

    const dataA = resultA.status === 'fulfilled' ? resultA.value : null;
    const dataB = resultB.status === 'fulfilled' ? resultB.value : null;

    if (!dataA && !dataB) {
      return res.status(502).json({
        error: 'All services unavailable',
        timestamp: new Date().toISOString(),
      });
    }

    res.json({
      serviceA: dataA,
      serviceB: dataB || { service: 'B', message: 'Service B unavailable (circuit breaker open)', status: 'degraded' },
      gateway: dataB ? 'ok' : 'degraded',
      timestamp: new Date().toISOString(),
    });

  } catch (err) {
    next(err);
  }
});

app.use((err, req, res, _next) => {
  console.error('[Gateway error]', err.message);
  res.status(502).json({
    error: 'One or more services failed after retries',
    details: err.message,
  });
});

app.listen(PORT, () => {
  console.log(`Gateway (RETRY + CIRCUIT BREAKER) running → http://localhost:${PORT}`);
  console.log(`  Retries: ${MAX_RETRIES}x per service, NO backoff, CIRCUIT BREAKER enabled`);
});
