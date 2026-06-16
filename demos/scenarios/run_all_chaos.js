const { spawn, execSync } = require('child_process');

const scenarios = [
  {
    label: 'CIRCUIT BREAKER: CASCADING TIMEOUT',
    starter: 'circuit_breaker_timeout.js',
    client: '../client/test-load.js',
    clientArgs: ['--requests', '20']
  },
  {
    label: 'CIRCUIT BREAKER: RETRY STORM',
    starter: 'circuit_breaker_cascade.js',
    client: '../client/test-load.js',
    clientArgs: ['--requests', '20']
  },
  {
    label: 'SAGA: ORPHANED ORDERS',
    starter: 'saga_lost_payment.js',
    client: '../client/test-orders.js',
    clientArgs: ['--orders', '10']
  },
  {
    label: 'SAGA: DOUBLE CHARGE',
    starter: 'saga_double_charge.js',
    client: '../client/test-orders.js',
    clientArgs: ['--orders', '10']
  }
];

async function runScenario(scenario) {
  console.log('\n' + '='.repeat(60));
  console.log(`🚀 SCENARIO: ${scenario.label}`);
  console.log('='.repeat(60));

  console.log(`[Runner] Starting environment: ${scenario.starter}...`);
  const starterProcess = spawn('node', [scenario.starter], {
    cwd: __dirname,
    stdio: 'inherit',
    shell: true
  });

  console.log(`[Runner] Waiting 5s for services to boot...`);
  await new Promise(resolve => setTimeout(resolve, 5000));

  console.log(`\n[Runner] Running client test: ${scenario.client}...`);
  try {
    execSync(`node ${scenario.client} ${scenario.clientArgs.join(' ')}`, {
      cwd: __dirname,
      stdio: 'inherit'
    });
  } catch (e) {
    console.error(`[Runner] Test execution failed.`);
  }

  console.log(`\n[Runner] Stopping scenario...`);

  if (process.platform === 'win32') {
    try {
      execSync(`taskkill /pid ${starterProcess.pid} /T /F`, { stdio: 'ignore' });
    } catch (e) { }
  } else {
    starterProcess.kill();
  }

  console.log(`[Runner] Waiting 3s for ports to clear...`);
  await new Promise(resolve => setTimeout(resolve, 3000));
}

async function main() {
  for (const scenario of scenarios) {
    await runScenario(scenario);
  }
  console.log('\n✅ All chaos scenarios completed.');
}

main();
