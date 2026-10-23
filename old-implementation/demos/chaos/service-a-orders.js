import express from 'express';

const app = express();
const PORT = process.env.PORT || 3001;

app.use(express.json());

const orders = [];
let nextId = 1;

app.post('/order', (req, res) => {
  const order = {
    id: nextId++,
    item: req.body.item || 'unknown-item',
    amount: req.body.amount || 0,
    status: 'created',
    createdAt: new Date().toISOString(),
  };
  orders.push(order);
  console.log(`ORDER CREATED: #${order.id} - ${order.item} ($${order.amount})`);
  res.status(201).json(order);
});

app.get('/orders', (_req, res) => {
  res.json({ total: orders.length, orders });
});

app.delete('/order/:id', (req, res) => {
  const id = parseInt(req.params.id, 10);
  const idx = orders.findIndex(o => o.id === id);
  if (idx === -1) {
    return res.status(404).json({ error: `Order #${id} not found` });
  }
  orders[idx].status = 'cancelled';
  console.log(`ORDER CANCELLED: #${id}`);
  res.json(orders[idx]);
});

app.get('/data', (_req, res) => {
  res.json({
    service: 'A',
    message: 'Hello from Service A!',
    value: Math.floor(Math.random() * 100),
    time: new Date().toISOString(),
  });
});

app.get('/health', (_, res) => res.send('ok'));

app.listen(PORT, () => {
  console.log(`Service A (ORDERS) running → http://localhost:${PORT}`);
  console.log('  POST /order, GET /orders, DELETE /order/:id');
});
