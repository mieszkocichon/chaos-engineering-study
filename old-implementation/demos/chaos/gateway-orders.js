import express from 'express';
import fetch from 'node-fetch';

const app = express();
const PORT = process.env.GATEWAY_PORT || 3000;
const RETRY = process.env.RETRY === 'true';
const TIMEOUT_MS = parseInt(process.env.TIMEOUT_MS, 10) || 5000;

app.use(express.json());

async function fetchWithTimeout(url, options = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);

  try {
    const res = await fetch(url, { ...options, signal: controller.signal });
    if (!res.ok) {
      const body = await res.text();
      throw new Error(`HTTP ${res.status}: ${body}`);
    }
    return await res.json();
  } finally {
    clearTimeout(timer);
  }
}

app.post('/api/order', async (req, res) => {
  const { item, amount } = req.body;
  console.log(`\n--- New order: ${item} $${amount} ---`);

  let order;
  try {
    order = await fetchWithTimeout('http://localhost:3001/order', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ item, amount }),
    });
    console.log(`  Step 1 OK: Order #${order.id} created in Service A`);
  } catch (err) {
    console.error(`  Step 1 FAILED: Could not create order → ${err.message}`);
    return res.status(500).json({ error: 'Failed to create order', details: err.message });
  }

  const attemptPayment = async () => {
    return fetchWithTimeout('http://localhost:3002/pay', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ orderId: order.id, amount }),
    });
  };

  let payment;
  try {
    payment = await attemptPayment();
    console.log(`  Step 2 OK: Payment #${payment.id} processed in Service B`);
  } catch (err) {
    console.error(`  Step 2 FAILED: Payment failed → ${err.message}`);

    if (RETRY) {

      console.log('  RETRYING entire flow (will cause duplicates)...');
      try {
        const order2 = await fetchWithTimeout('http://localhost:3001/order', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ item, amount }),
        });
        console.log(`  Retry: Order #${order2.id} created (DUPLICATE!)`);

        payment = await attemptPayment();
        console.log(`  Retry: Payment processed (may also be DUPLICATE!)`);
      } catch (retryErr) {
        console.error(`  Retry also FAILED: ${retryErr.message}`);

        return res.status(500).json({
          error: 'Order flow failed after retry',
          orphanedOrders: [order.id],
          details: retryErr.message,
        });
      }
    } else {

      return res.status(500).json({
        error: 'Payment failed — order is orphaned (no saga rollback!)',
        orphanedOrderId: order.id,
        details: err.message,
      });
    }
  }

  res.json({
    message: 'Order completed',
    order,
    payment,
    timestamp: new Date().toISOString(),
  });
});

app.get('/api/orders', async (_req, res) => {
  try {
    const data = await fetchWithTimeout('http://localhost:3001/orders');
    res.json(data);
  } catch (err) {
    res.status(502).json({ error: err.message });
  }
});

app.get('/api/payments', async (_req, res) => {
  try {
    const data = await fetchWithTimeout('http://localhost:3002/payments');
    res.json(data);
  } catch (err) {
    res.status(502).json({ error: err.message });
  }
});

app.get('/api/combine', async (req, res, next) => {
  try {
    const [dataA, dataB] = await Promise.all([
      fetchWithTimeout('http://localhost:3001/data'),
      fetchWithTimeout('http://localhost:3002/data'),
    ]);
    res.json({ serviceA: dataA, serviceB: dataB, gateway: 'ok', timestamp: new Date().toISOString() });
  } catch (err) {
    next(err);
  }
});

app.use((err, req, res, _next) => {
  console.error('[Gateway error]', err.message);
  res.status(502).json({ error: 'Service failure', details: err.message });
});

app.listen(PORT, () => {
  console.log(`Gateway (ORDERS) running → http://localhost:${PORT}`);
  console.log(`  RETRY: ${RETRY}, TIMEOUT: ${TIMEOUT_MS}ms`);
  console.log('  POST /api/order — no saga, no compensation!');
});
