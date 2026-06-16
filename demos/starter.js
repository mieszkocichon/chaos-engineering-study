const { spawn } = require('child_process');
const path = require('path');

const services = [
  { name: 'Gateway', dir: 'gateway', file: 'index.js', color: '\x1b[36m' },
  { name: 'Service A', dir: 'service-a', file: 'index.js', color: '\x1b[32m' },
  { name: 'Service B', dir: 'service-b', file: 'index.js', color: '\x1b[33m' },
];

const reset = '\x1b[0m';

console.log('\nStarting all microservices...\n');

services.forEach(svc => {
  const cwd = path.join(__dirname, svc.dir);

  const proc = spawn('node', [svc.file], {
    cwd,
    stdio: ['ignore', 'pipe', 'pipe']
  });

  const prefix = `${svc.color}[${svc.name}]${reset} `;

  proc.stdout.on('data', (data) => {
    console.log(prefix + data.toString().trim());
  });

  proc.stderr.on('data', (data) => {
    console.error(prefix + '\x1b[31m' + data.toString().trim() + reset);
  });

  proc.on('close', (code) => {
    console.log(`${prefix} process exited with code ${code}`);
  });

  proc.on('error', (err) => {
    console.error(`${prefix} Error starting: ${err.message}`);
  });

  console.log(`${prefix} started → PID ${proc.pid}`);
});

console.log('\nAll services should be running now.');
console.log('To test → open: http://localhost:3000/api/combine');
console.log('To stop → Ctrl+C in this terminal (child processes will be killed)\n');
