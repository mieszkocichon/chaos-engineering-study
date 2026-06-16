const { spawn } = require('child_process');
const path = require('path');

const services = [
  { name: 'Gateway',           dir: '../gateway', file: 'index.js',          color: '\x1b[36m' },
  { name: 'Service A (SLOW)',  dir: '../chaos',   file: 'service-a-slow.js', color: '\x1b[32m' },
  { name: 'Service B',         dir: '../service-b', file: 'index.js',        color: '\x1b[33m' },
];

const reset = '\x1b[0m';
console.log('\n⚡ CHAOS: Circuit Breaker — Cascading Timeout\n');
console.log('Service A will randomly delay responses by 5-15 seconds.');
console.log('Gateway has NO circuit breaker → all requests will suffer.\n');

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

console.log('\nTest with: node client/test-load.js');
console.log('Stop with: Ctrl+C\n');
