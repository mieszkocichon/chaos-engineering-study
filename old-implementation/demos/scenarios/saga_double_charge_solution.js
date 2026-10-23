const { spawn } = require('child_process');
const path = require('path');

const services = [
  {
    name: 'Gateway (SAGA+IDEM)',
    dir: '../chaos',
    file: 'gateway-orders-saga.js',
    color: '\x1b[36m',
    env: { RETRY: 'true', TIMEOUT_MS: '2000' },
  },
  {
    name: 'Service A (ORDERS)',
    dir: '../chaos',
    file: 'service-a-orders.js',
    color: '\x1b[32m',
    env: {},
  },
  {
    name: 'Service B (SLOW PAY)',
    dir: '../chaos',
    file: 'service-b-payments.js',
    color: '\x1b[33m',
    env: { SLOW_MODE: 'true' },
  },
];

const reset = '\x1b[0m';
console.log('\n✅ SOLUTION: Saga — Double Charge (Idempotency)\n');
console.log('Gateway sends Idempotency-Key. Service B should reject duplicates.\n');

services.forEach(svc => {
  const cwd = path.join(__dirname, svc.dir);
  const proc = spawn('node', [svc.file], {
    cwd,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, ...svc.env },
  });
  const prefix = `${svc.color}[${svc.name}]${reset} `;

  proc.stdout.on('data', d => console.log(prefix + d.toString().trim()));
  proc.stderr.on('data', d => console.error(prefix + '\x1b[31m' + d.toString().trim() + reset));
  proc.on('close', code => console.log(`${prefix}exited with code ${code}`));
  proc.on('error', err => console.error(`${prefix}Error: ${err.message}`));

  console.log(`${prefix}started → PID ${proc.pid}`);
});

console.log('\nTest with: node client/test-orders.js');