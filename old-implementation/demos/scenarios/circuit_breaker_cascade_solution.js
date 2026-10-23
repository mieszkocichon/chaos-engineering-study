const { spawn } = require('child_process');
const path = require('path');

const services = [
  { name: 'Gateway (CB)',       dir: '../chaos',     file: 'gateway-retry-cb.js', color: '\x1b[36m' },
  { name: 'Service A',          dir: '../service-a', file: 'index.js',           color: '\x1b[32m' },
  { name: 'Service B (FLAKY)',  dir: '../chaos',     file: 'service-b-flaky.js', color: '\x1b[33m' },
];

const reset = '\x1b[0m';
console.log('\n✅ SOLUTION: Circuit Breaker Enabled\n');

services.forEach(svc => {
  const cwd = path.join(__dirname, svc.dir);
  const proc = spawn('node', [svc.file], { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
  const prefix = `${svc.color}[${svc.name}]${reset} `;

  proc.stdout.on('data', d => console.log(prefix + d.toString().trim()));
  proc.stderr.on('data', d => console.error(prefix + '\x1b[31m' + d.toString().trim() + reset));
});

console.log('\nTest with: node client/test-load.js --save after-cb');