import { existsSync, readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import process from 'node:process';
import { setTimeout as wait } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = dirname(fileURLToPath(import.meta.url));
const scenarioPath = resolve(root, process.argv[2] ?? 'scenarios/test-1.json');
const scenario = JSON.parse(readFileSync(scenarioPath, 'utf8'));
const recoveryCycles = scenario.recovery.groups.flatMap((group) =>
  Array.from({ length: group.count }, (_, offset) => ({
    group: group.approver,
    approver: group.approver === 'alternate' ? (offset % 2 === 0 ? 'ios' : 'android') : group.approver,
  })),
).map((cycle, index) => ({ ...cycle, number: index + 1 }));
const recoveryShowTimeoutMs = scenario.recovery.showTimeoutMs ?? 45_000;
const iosRecoveryApprovalCycles = recoveryCycles.filter((cycle) => cycle.approver === 'ios').map((cycle) => cycle.number);
const androidRecoveryApprovalCycles = recoveryCycles.filter((cycle) => cycle.approver === 'android').map((cycle) => cycle.number);
const projectRoot = resolve(root, '..');
const coreRoot = projectRoot;
const webDirectory = resolve(root, scenario.web.directory);
const composeRoot = resolve(root, scenario.ios.composeRoot);
const iosProjectPath = resolve(composeRoot, 'iosApp/iosApp.xcodeproj');
const iosDerivedDataPath = resolve(root, '.derivedData/iosApp');
const androidEmulatorPath = '/Users/dmitrykuklin/Library/Android/sdk/emulator/emulator';
const serverContainer = scenario.server.container;
const serverImage = scenario.server.image;
const processes = [];
const watchedProcessOutputs = [];
let approvalCoordinator;

function startApprovalCoordinator(port = 5180) {
  const allowedApprovals = new Set();
  const server = createServer((request, response) => {
    const url = new URL(request.url, `http://${request.headers.host}`);
    if (url.pathname !== '/approval') {
      response.writeHead(404).end();
      return;
    }

    const key = `${url.searchParams.get('platform')}:${url.searchParams.get('cycle')}`;
    response.writeHead(200, { 'content-type': 'text/plain' });
    response.end(allowedApprovals.has(key) ? 'allowed' : 'waiting');
  });

  return new Promise((resolvePromise, reject) => {
    server.once('error', reject);
    server.listen(port, '0.0.0.0', () => resolvePromise({
      allow(platform, cycle) {
        allowedApprovals.add(`${platform}:${cycle}`);
      },
      close() {
        return new Promise((resolveClose) => server.close(resolveClose));
      },
    }));
  });
}
function run(command, args, options = {}) {
  const child = spawn(command, args, {
    cwd: options.cwd ?? projectRoot,
    env: {
      ...process.env,
      PATH: `/opt/homebrew/bin:/usr/local/bin:${process.env.PATH ?? ''}`,
      ...(options.env ?? {}),
    },
    stdio: options.stdio ?? 'inherit',
    shell: false,
  });
  processes.push(child);
  return child;
}

function runAndWait(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = run(command, args, { ...options, stdio: options.stdio ?? 'inherit' });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`${command} exited with ${code ?? signal}`));
    });
  });
}

function runAndCapture(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = run(command, args, { ...options, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString();
    });
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      if (code === 0) resolvePromise({ stdout, stderr });
      else reject(new Error(`${command} exited with ${code ?? signal}\n${stderr || stdout}`));
    });
  });
}

function watchProcessOutput(command, args, options = {}) {
  let stdout = '';
  let stderr = '';
  const markerWaiters = new Set();

  const child = run(command, args, { ...options, stdio: ['ignore', 'pipe', 'pipe'] });
  const processOutput = (chunk, sink) => {
    const text = chunk.toString();
    if (sink === 'stdout') stdout += text;
    else stderr += text;
    process[sink].write(text);
    const allOutput = `${stdout}\n${stderr}`;
    for (const waiter of markerWaiters) {
      if (allOutput.includes(waiter.marker)) {
        clearTimeout(waiter.timeout);
        markerWaiters.delete(waiter);
        waiter.resolve();
      }
    }
  };

  child.stdout.on('data', (chunk) => processOutput(chunk, 'stdout'));
  child.stderr.on('data', (chunk) => processOutput(chunk, 'stderr'));

  let processError;
  const result = new Promise((resolvePromise, reject) => {
    child.once('error', reject);
    child.once('exit', (code, signal) => {
      processError = code === 0 ? new Error(`${command} ended before expected E2E event`) : new Error(`${command} exited with ${code ?? signal}`);
      for (const waiter of markerWaiters) {
        clearTimeout(waiter.timeout);
        waiter.reject(processError);
      }
      markerWaiters.clear();
      if (code === 0) resolvePromise({ stdout, stderr });
      else reject(processError);
    });
  });

  const waitForMarker = (marker, timeoutMs = 300_000) => new Promise((resolvePromise, reject) => {
    if (`${stdout}\n${stderr}`.includes(marker)) return resolvePromise();
    if (processError) return reject(processError);
    const waiter = {
      marker,
      resolve: resolvePromise,
      reject: reject,
      timeout: setTimeout(() => {
        markerWaiters.delete(waiter);
        reject(new Error(`Timed out waiting for ${marker}`));
      }, timeoutMs),
    };
    markerWaiters.add(waiter);
  });

  const watched = {
    command,
    args,
    child,
    waitForMarker,
    result,
    outputTail: () => `${stdout}\n${stderr}`.slice(-12_000),
  };
  watchedProcessOutputs.push(watched);
  return watched;
}

function printFailureDiagnostics() {
  for (const watched of watchedProcessOutputs) {
    const tail = watched.outputTail();
    if (tail.trim()) {
      console.error(`\n=== E2E diagnostic: ${watched.command} ${watched.args.join(' ')} ===\n${tail}`);
    }
  }
}

async function waitForHttp(url, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok || response.status < 500) return;
    } catch (error) {
      lastError = error;
    }
    await wait(500);
  }
  throw new Error(`Timed out waiting for ${url}: ${lastError?.message ?? 'server unavailable'}`);
}

async function stopProcesses() {
  await approvalCoordinator?.close().catch(() => {});
  approvalCoordinator = undefined;
  for (const child of processes.reverse()) {
    if (!child.killed) child.kill('SIGTERM');
  }
  await wait(500);
  await runAndWait('docker', ['rm', '-f', serverContainer], { stdio: 'ignore' }).catch(() => {});
}

async function findIosSimulator() {
  const { stdout } = await runAndCapture('xcrun', ['simctl', 'list', 'devices', 'available', '-j']);
  const devicesByRuntime = JSON.parse(stdout).devices;
  const devices = Object.values(devicesByRuntime).flat();
  const device = devices.find((candidate) => candidate.name === scenario.ios.simulatorName);
  if (!device) throw new Error(`iOS simulator not found: ${scenario.ios.simulatorName}`);
  return device;
}

async function prepareIosSimulator() {
  const simulator = await findIosSimulator();
  console.log(`8. Preparing iOS simulator: ${simulator.name} (${simulator.udid})`);
  if (simulator.state !== 'Booted') {
    await runAndWait('xcrun', ['simctl', 'boot', simulator.udid]);
  }
  await runAndWait('xcrun', ['simctl', 'bootstatus', simulator.udid, '-b']);
  await runAndWait('xcrun', ['simctl', 'uninstall', simulator.udid, scenario.ios.bundleId], { stdio: 'ignore' }).catch(() => {});
  await runAndWait('open', ['-a', 'Simulator']).catch(() => {});
  return simulator.udid;
}

function startIosJoinTest(simulatorUdid) {
  console.log('9. Starting iOS UI test');
  return watchProcessOutput(
    'xcodebuild',
    [
      'test',
      '-project',
      iosProjectPath,
      '-scheme',
      'iosApp',
      '-configuration',
      'Debug',
      '-destination',
      `id=${simulatorUdid}`,
      '-derivedDataPath',
      iosDerivedDataPath,
      '-only-testing:iosAppUITests/CaseOneJoinFromIosUITest/testJoinApprovedVaultAndShowSecret',
    ],
    {
      cwd: resolve(composeRoot, 'iosApp'),
      env: {
        E2E_VAULT_NAME: scenario.vault.name,
        E2E_SECRET_NAME: scenario.secret.name,
        E2E_SECRET_VALUE: scenario.secret.value,
        E2E_RECOVERY_CYCLES: String(recoveryCycles.length),
        E2E_IOS_RECOVERY_APPROVALS: iosRecoveryApprovalCycles.join(','),
        E2E_APPROVAL_COORDINATOR_URL: 'http://127.0.0.1:5180',
      },
    },
  );
}

async function prepareAndroidEmulator() {
  const { stdout } = await runAndCapture('adb', ['devices']);
  let serial = stdout.split('\n').map((line) => line.trim().split(/\s+/)).find(([id, state]) => id.startsWith('emulator-') && state === 'device')?.[0];

  if (!serial) {
    console.log(`12. Starting Android emulator: ${scenario.android.avdName}`);
    run(androidEmulatorPath, ['-avd', scenario.android.avdName]);
    await runAndWait('adb', ['wait-for-device']);
    const devices = await runAndCapture('adb', ['devices']);
    serial = devices.stdout.split('\n').map((line) => line.trim().split(/\s+/)).find(([id, state]) => id.startsWith('emulator-') && state === 'device')?.[0];
  }

  if (!serial) throw new Error(`Android emulator ${scenario.android.avdName} did not start`);
  console.log(`13. Preparing Android emulator: ${serial}`);
  await waitForAndroidBoot(serial);
  await runAndWait('adb', ['-s', serial, 'uninstall', scenario.android.bundleId], { stdio: 'ignore' }).catch(() => {});
  return serial;
}

async function waitForAndroidBoot(serial, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = 'Android did not report sys.boot_completed=1';

  while (Date.now() < deadline) {
    try {
      const { stdout } = await runAndCapture('adb', ['-s', serial, 'shell', 'getprop', 'sys.boot_completed']);
      if (stdout.trim() === '1') return;
      lastError = `sys.boot_completed=${stdout.trim() || '<empty>'}`;
    } catch (error) {
      lastError = error.message;
    }
    await wait(1_000);
  }

  throw new Error(`Android emulator ${serial} did not finish booting: ${lastError}`);
}

async function runAdbWithRetry(args, { attempts = 5, delayMs = 1_000 } = {}) {
  let lastError;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return await runAndCapture('adb', args);
    } catch (error) {
      lastError = error;
      console.warn(`ADB command failed (${attempt}/${attempts}): adb ${args.join(' ')}\n${error.message}`);
      await runAndCapture('adb', ['reconnect', 'device']).catch((reconnectError) => {
        console.warn(`ADB reconnect failed: ${reconnectError.message}`);
      });
      if (attempt < attempts) await wait(delayMs);
    }
  }
  throw lastError;
}

async function startAndroidJoinTest(serial) {
  console.log('14. Starting Android UI test');
  await runAdbWithRetry(['-s', serial, 'logcat', '-c']);
  const logcat = watchProcessOutput('adb', ['-s', serial, 'logcat', 'MetaSecretE2E:I', '*:S']);
  void logcat.result.catch(() => {}); // logcat is stopped deliberately when the instrumentation test ends
  const test = runAndWait(
    './gradlew',
    [
      ':composeApp:connectedDebugAndroidTest',
      '-Pandroid.testInstrumentationRunnerArguments.class=metasecret.project.com.CaseOneJoinFromAndroidTest',
      `-Pandroid.testInstrumentationRunnerArguments.vaultName=${scenario.vault.name}`,
      `-Pandroid.testInstrumentationRunnerArguments.secretName=${scenario.secret.name}`,
      `-Pandroid.testInstrumentationRunnerArguments.recoveryCycles=${recoveryCycles.length}`,
      `-Pandroid.testInstrumentationRunnerArguments.androidRecoveryApprovals=${androidRecoveryApprovalCycles.join(',')}`,
      '-Pandroid.testInstrumentationRunnerArguments.approvalCoordinatorUrl=http://10.0.2.2:5180',
    ],
    { cwd: composeRoot, env: { ANDROID_SERIAL: serial } },
  );
  const result = test.finally(() => logcat.child.kill('SIGTERM'));
  return { ...logcat, result };
}

async function setupVirtualAuthenticator(page) {
  const cdp = await page.context().newCDPSession(page);
  await cdp.send('WebAuthn.enable');
  await cdp.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  });
}

async function unlockWithPasskeyIfNeeded(page) {
  const createPasskeyButton = page.getByRole('button', { name: 'Create Passkey' });
  const authenticateButton = page.getByRole('button', { name: 'Authenticate with Passkey' });

  await Promise.race([
    page.getByPlaceholder('vault name').waitFor({ state: 'visible' }),
    createPasskeyButton.waitFor({ state: 'visible' }),
    authenticateButton.waitFor({ state: 'visible' }),
  ]);

  if (await createPasskeyButton.isVisible()) {
    console.log('5a. Creating test passkey');
    await createPasskeyButton.click();
  } else if (await authenticateButton.isVisible()) {
    console.log('5a. Authenticating with test passkey');
    await authenticateButton.click();
  }

  await page.getByPlaceholder('vault name').waitFor({ state: 'visible' });
}

async function approveJoinRequestOnWeb(page, deviceName) {
  console.log(`Web approving ${deviceName} join request`);
  await page.getByRole('link', { name: 'Devices', exact: true }).click();
  await page.getByTestId('pending-device-row').waitFor({ state: 'visible', timeout: 120_000 });
  await page.getByTestId('pending-device-row').click();
  await page.getByTestId('accept-join-request').click();
  await page.getByTestId('pending-device-row').waitFor({ state: 'detached', timeout: 120_000 }).catch(() => {});
}

async function closeWebSecret(page) {
  const closeButton = page.getByRole('button', { name: /close/i }).first();
  if (await closeButton.isVisible().catch(() => false)) {
    await closeButton.click();
    await page.getByText(scenario.secret.value, { exact: true }).waitFor({ state: 'hidden' }).catch(() => {});
  }
}

async function runRecoveryCycles(page, iosTest, androidTest) {
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();
  const recoverySecretRow = page.getByRole('listitem').filter({ hasText: scenario.secret.name });

  for (const cycle of recoveryCycles) {
    console.log(`15.${cycle.number} Web requesting secret recovery (${cycle.approver} approves)`);
    await recoverySecretRow.getByRole('button', { name: 'Recover', exact: true }).click();
    await Promise.all([
      iosTest.waitForMarker(`E2E: IOS_RECOVERY_REQUEST_ALERT_${cycle.number}`),
      androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_REQUEST_ALERT_${cycle.number}`),
    ]);

    approvalCoordinator.allow(cycle.approver, cycle.number);
    const approverTest = cycle.approver === 'ios' ? iosTest : androidTest;
    const approverMarker = cycle.approver === 'ios'
      ? `E2E: IOS_RECOVERY_APPROVE_SUCCESS_${cycle.number}`
      : `E2E: ANDROID_RECOVERY_APPROVE_SUCCESS_${cycle.number}`;
    await approverTest.waitForMarker(approverMarker);
    await page.getByText(scenario.secret.value, { exact: true }).waitFor({ timeout: recoveryShowTimeoutMs });
    console.log(`✅ Recovery ${cycle.number}/${recoveryCycles.length}: Web show secret passed`);
    await closeWebSecret(page);
    await Promise.all([
      iosTest.waitForMarker(`E2E: IOS_RECOVERY_REQUEST_CLOSED_${cycle.number}`),
      androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_REQUEST_CLOSED_${cycle.number}`),
    ]);

    if (cycle.number < recoveryCycles.length) {
      await wait(scenario.recovery.pauseMs);
    }
  }
}

async function main() {
  if (!existsSync(webDirectory)) throw new Error(`Web directory not found: ${webDirectory}`);
  if (!existsSync(composeRoot)) throw new Error(`Compose directory not found: ${composeRoot}`);

  console.log(`\n=== ${scenario.name} ===`);
  approvalCoordinator = await startApprovalCoordinator();
  console.log('1. Rebuilding the Web WASM package');
  await runAndWait('/opt/homebrew/bin/task', ['wasm-local'], { cwd: coreRoot });

  console.log('2. Building local server image');
  await runAndWait('/opt/homebrew/bin/task', ['meta-server'], { cwd: coreRoot });

  console.log('3. Cleaning server state');
  await runAndWait('docker', ['rm', '-f', serverContainer], { stdio: 'ignore' }).catch(() => {});
  console.log('4. Starting local server');
  const server = watchProcessOutput('docker', ['run', '--rm', '--name', serverContainer, '-p', `${scenario.server.port}:3000`, serverImage]);
  void server.result.catch(() => {});
  await waitForHttp(scenario.server.url);

  console.log('5. Starting Web in a visible browser');
  run('npm', ['run', 'dev', '--', '--host', 'localhost', '--port', '5173'], { cwd: webDirectory });
  await waitForHttp(scenario.web.url);

  const browser = await chromium.launch({ headless: false, slowMo: 250 });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  await setupVirtualAuthenticator(page);

  console.log('5. Creating Vault');
  await page.goto(scenario.web.url, { waitUntil: 'networkidle' });
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').fill(scenario.vault.name);
  await page.getByRole('button', { name: 'Set Vault Name' }).click();
  await page.getByText('Vault name is free!').waitFor();
  await page.getByRole('button', { name: 'Create', exact: true }).click();

  console.log('6. Creating Secret');
  await page.getByRole('button', { name: '+ Add Secret' }).waitFor();
  await page.getByRole('button', { name: '+ Add Secret' }).click();
  await page.getByPlaceholder('Secret name').fill(scenario.secret.name);
  await page.getByPlaceholder('Enter your secret').fill(scenario.secret.value);
  await page.getByRole('button', { name: 'Add Secret', exact: true }).click();

  console.log('7. Showing Secret');
  const secretRow = page.getByRole('listitem').filter({ hasText: scenario.secret.name });
  await secretRow.getByRole('button', { name: 'Show', exact: true }).click();
  await page.getByText(scenario.secret.value, { exact: true }).waitFor();
  console.log('✅ Test #1 Web part passed');
  console.log('7a. Closing Web secret');
  await closeWebSecret(page);

  const simulatorUdid = await prepareIosSimulator();
  const iosTest = startIosJoinTest(simulatorUdid);
  await Promise.race([
    iosTest.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT'),
    iosTest.result.then(() => {
      throw new Error('iOS UI test finished before join request marker');
    }),
  ]);
  await approveJoinRequestOnWeb(page, 'iOS');
  await iosTest.waitForMarker('E2E: IOS_CLOSE_SECRET_SUCCESS');

  console.log('11. Web showing Secret after iOS joined');
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();
  await page.getByRole('button', { name: '+ Add Secret' }).waitFor();
  const updatedSecretRow = page.getByRole('listitem').filter({ hasText: scenario.secret.name });
  await updatedSecretRow.getByRole('button', { name: 'Show', exact: true }).click();
  await page.getByText(scenario.secret.value, { exact: true }).waitFor({ timeout: 120_000 });
  console.log('✅ Web show secret after iOS join passed');
  console.log('12. Closing Web secret');
  await closeWebSecret(page);

  const androidSerial = await prepareAndroidEmulator();
  const androidTest = await startAndroidJoinTest(androidSerial);
  await androidTest.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT');
  await iosTest.waitForMarker('E2E: IOS_ANDROID_JOIN_NOTIFICATION_SUCCESS', 120_000);
  await approveJoinRequestOnWeb(page, 'Android');
  await androidTest.waitForMarker('E2E: ANDROID_SECRET_VISIBLE');

  await runRecoveryCycles(page, iosTest, androidTest);
  await Promise.all([iosTest.result, androidTest.result]);

  console.log('✅ Test #1 Web + iOS + Android part passed');
  console.log('Browser remains open. Press Ctrl+C when ready to stop.');
  await new Promise(() => {});
}

process.once('SIGINT', async () => {
  await stopProcesses();
  process.exit(130);
});
process.once('SIGTERM', async () => {
  await stopProcesses();
  process.exit(143);
});

main().catch(async (error) => {
  console.error(`\n❌ Test #1 failed: ${error.message}`);
  printFailureDiagnostics();
  await stopProcesses();
  process.exit(1);
});
