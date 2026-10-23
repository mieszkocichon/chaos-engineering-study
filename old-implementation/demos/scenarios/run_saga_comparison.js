const { spawn, execSync } = require('child_process');

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

  console.log(`\n[Runner] Running orders test (saving to ${saveFile})...`);
  try {

    execSync(`node ../client/test-orders.js --orders 20 --save ${saveFile}`, {
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
    'MEASURE "BEFORE" (No Saga)',
    'saga_lost_payment.js',
    'before-saga'
  );

  await runStep(
    'MEASURE "AFTER" (With Saga)',
    'saga_lost_payment_solution.js',
    'after-saga'
  );

  console.log('\n📊 COMPARISON REPORT (Consistency Check):');
  execSync(`node ../client/compare.js ../results/before-saga.json ../results/after-saga.json`, { stdio: 'inherit' });
}

main();
