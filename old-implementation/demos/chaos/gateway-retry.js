import express from 'express';
import fetch from 'node-fetch';

const app = express();
const PORT = process.env.GATEWAY_PORT || 3000;
const MAX_RETRIES = 3;

const services = {
  a: 'http://localhost:3001/data',
  b: 'http://localhost:3002/data',
};

async function fetchWithRetry(url, retries = MAX_RETRIES) {
  for (let attempt = 1; attempt <= retries; attempt++) {
    try {
      const res = await fetch(url);
      if (!res.ok) {
        throw new Error(`HTTP ${res.status} – ${res.statusText}`);
      }
      return await res.json();
    } catch (err) {
      console.log(`  RETRY ${attempt}/${retries} for ${url} → ${err.message}`);
      if (attempt === retries) {
        throw err;
      }

    }
  }
}

app.get('/api/combine', async (req, res, next) => {
  console.log('\n--- New request to /api/combine ---');
  try {
    const [dataA, dataB] = await Promise.all([
      fetchWithRetry(services.a),
      fetchWithRetry(services.b),
    ]);

    res.json({
      serviceA: dataA,
      serviceB: dataB,
      gateway: 'ok',
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
  console.log(`Gateway (RETRY STORM) running → http://localhost:${PORT}`);
  console.log(`  Retries: ${MAX_RETRIES}x per service, NO backoff, NO circuit breaker`);
});
