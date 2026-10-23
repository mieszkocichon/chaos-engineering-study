import express from 'express';

const app = express();
const PORT = Number(process.env.PORT) || 3002;

app.get('/data', (_req, res) => {
  const payload = {
    service: 'B',
    message: 'Hello from Service B',
    status: 'active',
    random: parseFloat(Math.random().toFixed(3)),
  };
  res.json(payload);
});

app.get('/health', (_req, res) => res.send('ok'));

app.use((req, res) => {
  res.status(404).json({ error: 'Service B - not found' });
});

app.use((err, _req, res, _next) => {
  console.error('Service B error:', err);
  res.status(500).json({ error: 'Internal Server Error' });
});

app.listen(PORT, () => {
  console.log(`Service B running → http://localhost:${PORT}/data`);
});
