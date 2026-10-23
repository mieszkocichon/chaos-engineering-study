import express from 'express';

const app = express();
const PORT = process.env.PORT || 3001;

app.use(express.json());


app.get('/data', (req, res) => {
  const payload = {
    service: 'A',
    message: 'Hello from Service A!',
    value: Math.floor(Math.random() * 100),
    time: new Date().toISOString()
  };
  res.json(payload);
});

app.get('/health', (_, res) => res.send('ok'));

app.use((req, res) => {
  res.status(404).json({ error: 'Service A - not found' });
});

app.use((err, req, res, next) => {
  console.error('Error:', err);
  res.status(500).json({ error: 'Internal Server Error' });
});

app.listen(PORT, () => {
  console.log(`Service A running → http://localhost:${PORT}/data`);
});
