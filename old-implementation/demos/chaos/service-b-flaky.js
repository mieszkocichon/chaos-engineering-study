import express from 'express';

const app = express();
const PORT = Number(process.env.PORT) || 3002;

let totalRequests = 0;
let failedRequests = 0;

app.get('/data', (_req, res) => {
  totalRequests++;
  const shouldFail = Math.random() < 0.6;

  if (shouldFail) {
    failedRequests++;
    console.log(`CHAOS: Returning 500! (failed ${failedRequests}/${totalRequests} = ${((failedRequests/totalRequests)*100).toFixed(0)}%)`);
    return res.status(500).json({ error: 'Internal Server Error - service unstable' });
  }

  res.json({
    service: 'B',
    message: 'Hello from Service B',
    status: 'active',
    random: parseFloat(Math.random().toFixed(3)),
  });
});

app.get('/health', (_req, res) => res.send('ok'));

app.listen(PORT, () => {
  console.log(`Service B (FLAKY CHAOS) running → http://localhost:${PORT}/data`);
  console.log('  ~60% of requests will return 500');
});
