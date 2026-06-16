import express from 'express';

const app = express();
const PORT = Number(process.env.PORT) || 3002;
const SLOW_MODE = process.env.SLOW_MODE === 'true';

app.use(express.json());

const payments = [];
let nextId = 1;

app.post('/pay', async (req, res) => {
  const paymentId = nextId++;

  if (SLOW_MODE) {
    const delayMs = 3000 + Math.floor(Math.random() * 2000);
    console.log(`CHAOS: Payment #${paymentId} processing slowly (${(delayMs/1000).toFixed(1)}s)...`);
    await new Promise(resolve => setTimeout(resolve, delayMs));
  }

  const shouldFail = !SLOW_MODE && Math.random() < 0.4;

  if (shouldFail) {
    console.log(`CHAOS: Payment #${paymentId} FAILED (random failure)`);
    return res.status(500).json({ error: 'Payment processing failed' });
  }

  const payment = {
    id: paymentId,
    orderId: req.body.orderId,
    amount: req.body.amount || 0,
    status: 'completed',
    processedAt: new Date().toISOString(),
  };
  payments.push(payment);
  console.log(`PAYMENT OK: #${paymentId} for order #${payment.orderId} ($${payment.amount})`);
  res.status(201).json(payment);
});

app.get('/payments', (_req, res) => {
  res.json({ total: payments.length, payments });
});

app.get('/data', (_req, res) => {
  res.json({
    service: 'B',
    message: 'Hello from Service B',
    status: 'active',
    random: parseFloat(Math.random().toFixed(3)),
  });
});

app.get('/health', (_req, res) => res.send('ok'));

app.listen(PORT, () => {
  console.log(`Service B (PAYMENTS) running → http://localhost:${PORT}`);
  console.log(`  SLOW_MODE: ${SLOW_MODE}`);
  if (SLOW_MODE) {
    console.log('  Payments will take 3-5s (causes gateway timeout)');
  } else {
    console.log('  ~40% of payments will randomly fail');
  }
});
