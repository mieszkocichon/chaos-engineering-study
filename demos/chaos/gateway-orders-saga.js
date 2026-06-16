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

async function cancelOrder(orderId) {
  try {
    await fetchWithTimeout(`http://localhost:3001/order/${orderId}`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
    });
    console.log(`  SAGA COMPENSATION: Order #${orderId} cancelled`);
  } catch (err) {
    console.error(`  SAGA COMPENSATION FAILED: Could not cancel order #${orderId}: ${err.message}`);
  }
}

async function sagaCreateOrderAndPay(item, amount) {

  const order = await fetchWithTimeout('http://localhost:3001/order', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ item, amount }),
  });
  console.log(`  Step 1 OK: Order #${order.id} created in Service A`);

  const maxPaymentRetries = RETRY ? 2 : 0;
  let attempts = 0;
  const idempotencyKey = `pay_order_${order.id}`;

  while (attempts <= maxPaymentRetries) {
    try {
      const payment = await fetchWithTimeout('http://localhost:3002/pay', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Idempotency-Key': idempotencyKey },
        body: JSON.stringify({ orderId: order.id, amount }),
      });
      console.log(`  Step 2 OK: Payment #${payment.id} processed in Service B`);
      return { order, payment };
    } catch (paymentErr) {
      attempts++;
      if (attempts > maxPaymentRetries) {
        console.error(`  Step 2 FAILED: Payment failed → ${paymentErr.message}`);

        await cancelOrder(order.id);
        throw new Error(`Payment failed after ${attempts} attempt(s), order rolled back: ${paymentErr.message}`);
      } else {
        console.warn(`  Payment retry #${attempts} failed: ${paymentErr.message}`);
      }
    }
  }
}

app.post('/api/order', async (req, res) => {
  const { item, amount } = req.body;
  console.log(`\n--- New order: ${item} $${amount} ---`);

  try {
    const { order, payment } = await sagaCreateOrderAndPay(item, amount);
    res.json({
      message: 'Order completed',
      order,
      payment,
      timestamp: new Date().toISOString(),
    });
  } catch (err) {
    console.error(`  Order saga failed: ${err.message}`);
    res.status(500).json({ error: err.message });
  }
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

app.use((err, req, res, _next) => {
  console.error('[Gateway error]', err.message);
  res.status(502).json({ error: 'Service failure', details: err.message });
});

app.listen(PORT, () => {
  console.log(`Gateway (SAGA) running → http://localhost:${PORT}`);
  console.log(`  RETRY: ${RETRY}, TIMEOUT: ${TIMEOUT_MS}ms`);
  console.log('  POST /api/order — with saga compensation!');
});
