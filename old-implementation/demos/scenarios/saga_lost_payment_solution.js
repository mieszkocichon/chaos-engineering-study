const { spawn } = require('child_process');
const path = require('path');

const services = [
  { name: 'Gateway (SAGA)',        dir: '../chaos', file: 'gateway-orders-saga.js', color: '\x1b[36m' },
  { name: 'Service A (ORDERS)',    dir: '../chaos', file: 'service-a-orders.js',    color: '\x1b[32m' },
  { name: 'Service B (PAYMENTS)',  dir: '../chaos', file: 'service-b-payments.js',  color: '\x1b[33m' },
];

const reset = '\x1b[0m';
console.log('\n✅ SOLUTION: Saga — Orphaned Orders (Compensating Transaction)\n');
console.log('Gateway will now ROLLBACK orders if payment fails.\n');

services.forEach(svc => {
  const cwd = path.join(__dirname, svc.dir);
  const proc = spawn('node', [svc.file], { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
  const prefix = `${svc.color}[${svc.name}]${reset} `;

  proc.stdout.on('data', d => console.log(prefix + d.toString().trim()));
  proc.stderr.on('data', d => console.error(prefix + '\x1b[31m' + d.toString().trim() + reset));
  proc.on('close', code => console.log(`${prefix}exited with code ${code}`));
  proc.on('error', err => console.error(`${prefix}Error: ${err.message}`));

  console.log(`${prefix}started → PID ${proc.pid}`);
});

console.log('\nTest with: node client/test-orders.js');