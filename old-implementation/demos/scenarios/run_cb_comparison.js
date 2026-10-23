const { spawn, execSync } = require('child_process');
const path = require('path');

async function runStep(label, starterFile, saveFile) {
  console.log('\n' + '='.repeat(60));
  console.log(`🚀 STEP: ${label}`);
  console.log('='.repeat(60));

  console.log(`[Runner] Starting environment: ${starterFile}...`);

  const starterProcess = spawn('node', [starterFile], {
    cwd: __dirname,
    stdio: 'inherit',
    shell: true
  });

  console.log(`[Runner] Waiting 5s for services to boot...`);
  await new Promise(resolve => setTimeout(resolve, 5000));

  console.log(`\n[Runner] Running load test (saving to ${saveFile})...`);
  try {

    execSync(`node ../client/test-load.js --requests 200 --save ${saveFile}`, {
      cwd: __dirname,
      stdio: 'inherit'
    });
  } catch (e) {
    console.error(`[Runner] Test execution failed.`);
  }

  console.log(`\n[Runner] Stopping environment...`);

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

  await runStep(
    'MEASURE "BEFORE" (Retry Storm)',
    'circuit_breaker_cascade.js',
    'before-cb'
  );

  await runStep(
    'MEASURE "AFTER" (Circuit Breaker)',
    'circuit_breaker_cascade_solution.js',
    'after-cb'
  );

  console.log('\n' + '='.repeat(60));
  console.log(`📊 COMPARISON REPORT`);
  console.log('='.repeat(60));
  try {
    execSync(`node ../client/compare.js ../results/before-cb.json ../results/after-cb.json`, {
      cwd: __dirname,
      stdio: 'inherit'
    });
  } catch (e) {
    console.error('Failed to run comparison script.');
  }
}

main();
