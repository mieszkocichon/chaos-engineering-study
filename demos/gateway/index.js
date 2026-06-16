import express from 'express';
import fetch from 'node-fetch';

const app = express();
const PORT = process.env.GATEWAY_PORT || 3000;

const services = {
  a: 'http://localhost:3001/data',
  b: 'http://localhost:3002/data',
};

const SERVICE_TIMEOUT_MS = parseInt(process.env.SERVICE_TIMEOUT_MS, 10) || 3000;

async function fetchWithTimeout(url) {
  const res = await fetch(url);

  if (!res.ok) {
    throw new Error(`HTTP ${res.status} – ${res.statusText}`);
  }

  return await res.json();
}

app.get('/api/combine', async (req, res, next) => {
  try {
    const [dataA, dataB] = await Promise.all([
      fetchWithTimeout(services.a),
      fetchWithTimeout(services.b),
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

app.use((err, req, res, next) => {
  console.error('[Gateway error]', err);

  const status = err.name === 'AbortError' ? 504 : 502;

  res.status(status).json({
    error: 'One or more services failed',
    details: err.message,
  });
});

app.listen(PORT, () => {
  console.log(`Gateway running → http://localhost:${PORT}`);
});
