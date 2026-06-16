import express from 'express';

const app = express();
const PORT = process.env.PORT || 3001;

app.use(express.json());

app.get('/data', async (req, res) => {
  const shouldDelay = Math.random() < 0.5;

  if (shouldDelay) {
    const delayMs = 5000 + Math.floor(Math.random() * 10000);
    console.log(`CHAOS: Adding ${(delayMs / 1000).toFixed(1)}s delay to response...`);
    await new Promise(resolve => setTimeout(resolve, delayMs));
  }

  res.json({
    service: 'A',
    message: 'Hello from Service A!',
    value: Math.floor(Math.random() * 100),
    time: new Date().toISOString(),
    delayed: shouldDelay,
  });
});

app.get('/health', (_, res) => res.send('ok'));

app.use((req, res) => {
  res.status(404).json({ error: 'Service A - not found' });
});

app.listen(PORT, () => {
  console.log(`Service A (SLOW CHAOS) running → http://localhost:${PORT}/data`);
  console.log('  ~50% of requests will have 5-15s random delay');
});
