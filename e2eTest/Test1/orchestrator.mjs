import { ensureIosFfi, verifyIosXcodeBuild } from '../shared/ios-ffi-provenance.mjs';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import process from 'node:process';
import { setTimeout as wait } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { discoverAndroidSdk, findOnlineAndroidAvd, withAndroidSdkPath } from '../shared/android-sdk.mjs';
import { installAndVerifyAndroidE2eApps, prepareIosTestApps } from '../shared/platform-readiness.mjs';
import { openVisibleIosDeviceUi } from '../shared/visible-ios-ui.mjs';

const root = dirname(fileURLToPath(import.meta.url));
const e2eRoot = resolve(root, '..');
const scenarioArgument = process.argv.slice(2).find((argument) => !argument.startsWith('--'));
const scenarioPath = resolve(root, scenarioArgument ?? 'scenario.json');
const scenario = JSON.parse(readFileSync(scenarioPath, 'utf8'));
const exitOnSuccess = process.argv.includes('--exit-on-success') || process.env.E2E_EXIT_ON_SUCCESS === '1';
const recoveryCycles = scenario.recovery.groups.flatMap((group) =>
  Array.from({ length: group.count }, (_, offset) => ({
    group: group.approver,
    approver: group.approver === 'alternate' ? (offset % 2 === 0 ? 'ios' : 'android') : group.approver,
  })),
).map((cycle, index) => ({ ...cycle, number: index + 1 }));
const recoveryShowTimeoutMs = scenario.recovery.showTimeoutMs ?? 45_000;
const iosRecoveryApprovalCycles = recoveryCycles.filter((cycle) => cycle.approver === 'ios').map((cycle) => cycle.number);
const androidRecoveryApprovalCycles = recoveryCycles.filter((cycle) => cycle.approver === 'android').map((cycle) => cycle.number);
const projectRoot = resolve(e2eRoot, '..');
const coreRoot = projectRoot;
const webDirectory = resolve(e2eRoot, scenario.web.directory);
const composeRoot = resolve(e2eRoot, scenario.ios.composeRoot);
const iosProjectPath = resolve(composeRoot, 'iosApp/iosApp.xcodeproj');
const iosDerivedDataPath = resolve(e2eRoot, '.derivedData/iosApp');
const artifactsDirectory = resolve(e2eRoot, '.artifacts');
const testAttemptToken = process.env.E2E_RUN_ID
  ? `${process.env.E2E_RUN_ID}-attempt-${process.env.E2E_TEST_ATTEMPT ?? '1'}`
  : new Date().toISOString().replace(/[:.]/g, '-');
const iosResultBundlePath = resolve(artifactsDirectory, `test-1-run-${testAttemptToken}.xcresult`);
const androidSdkTools = discoverAndroidSdk();
const androidEmulatorPath = androidSdkTools.emulatorPath;
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
      PATH: withAndroidSdkPath(`/opt/homebrew/bin:/usr/local/bin:${process.env.PATH ?? ''}`, androidSdkTools),
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
  const closed = new Promise((resolvePromise) => child.once('close', resolvePromise));

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
    closed,
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
  if (process.env.E2E_RANGE_RUN === '1' && simulator.state !== 'Booted') {
    throw new Error(`Range supervisor's iOS simulator ${simulator.name} is not Booted at scenario handoff`);
  }
  if (simulator.state !== 'Booted') {
    await runAndWait('xcrun', ['simctl', 'boot', simulator.udid]);
  }
  await runAndWait('xcrun', ['simctl', 'bootstatus', simulator.udid, '-b']);
  if (process.env.E2E_RANGE_RUN !== '1') {
    const installed = await runAndWait('xcrun', ['simctl', 'get_app_container', simulator.udid, scenario.ios.bundleId])
      .then(() => true, (error) => {
        if (/not installed|No such file|not found|No matching applications/i.test(error.message)) return false;
        throw error;
      });
    if (installed) await runAndWait('xcrun', ['simctl', 'uninstall', simulator.udid, scenario.ios.bundleId]);
  }
  await openVisibleIosDeviceUi({ runAndCapture, runAndWait, simulatorUdid: simulator.udid });
  return simulator.udid;
}

function startIosJoinTest(simulatorUdid) {
  verifyIosXcodeBuild({ derivedDataPath: iosDerivedDataPath });
  console.log('9. Starting iOS UI test');
  const watched = watchProcessOutput(
    'xcodebuild',
    [
      'test-without-building',
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
      '-resultBundlePath',
      iosResultBundlePath,
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
  // XCTest can pass the selected UI test while xcodebuild's testmanagerd
  // session remains open. Treat the selected-suite pass marker as the
  // authoritative UI result, then close the lingering wrapper before the
  // orchestrator advances to the next recovery cycle.
  const selectedSuitePassed = watched.waitForMarker(
    "Test Suite 'Selected tests' passed",
    600_000,
  ).then(async () => {
    if (!watched.child.killed) watched.child.kill('SIGTERM');
    await watched.closed;
    return { stdout: watched.outputTail(), stderr: '' };
  });
  void watched.result.catch(() => {});
  void selectedSuitePassed.catch(() => {});
  return { ...watched, result: selectedSuitePassed };
}

async function prepareAndroidEmulator() {
  let serial = await findOnlineAndroidAvd(androidSdkTools, scenario.android.avdName, runAndCapture);

  if (process.env.E2E_RANGE_RUN === '1') {
    if (!serial) throw new Error(`Range supervisor's Android AVD ${scenario.android.avdName} is offline at scenario handoff`);
    await waitForAndroidBoot(serial);
    return serial;
  }

  if (!serial) {
    console.log(`12. Starting Android emulator: ${scenario.android.avdName}`);
    run(androidEmulatorPath, ['-avd', scenario.android.avdName]);
    serial = await waitForConfiguredAndroidAvd();
  }

  if (!serial) throw new Error(`Android emulator ${scenario.android.avdName} did not start`);
  console.log(`13. Preparing Android emulator: ${serial}`);
  await waitForAndroidBoot(serial);
  const { stdout: installedPath } = await runAndCapture('adb', ['-s', serial, 'shell', 'pm', 'path', scenario.android.bundleId]);
  if (installedPath.includes('package:')) {
    await runAndWait('adb', ['-s', serial, 'uninstall', scenario.android.bundleId]);
    const { stdout: remainingPath } = await runAndCapture('adb', ['-s', serial, 'shell', 'pm', 'path', scenario.android.bundleId]);
    if (remainingPath.includes('package:')) throw new Error(`Android app ${scenario.android.bundleId} remains installed after uninstall`);
  }
  return serial;
}

async function waitForConfiguredAndroidAvd(timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = `AVD ${scenario.android.avdName} is not online`;
  while (Date.now() < deadline) {
    try {
      const serial = await findOnlineAndroidAvd(androidSdkTools, scenario.android.avdName, runAndCapture);
      if (serial) return serial;
    } catch (error) {
      lastError = error.message;
    }
    await wait(1_000);
  }
  throw new Error(`Android AVD ${scenario.android.avdName} did not become online: ${lastError}`);
}

async function preflightLocalRuntimes() {
  const { stdout } = await runAndCapture(androidEmulatorPath, ['-list-avds']);
  const avds = stdout.split(/\r?\n/).map((name) => name.trim()).filter(Boolean);
  if (!avds.includes(scenario.android.avdName)) {
    throw new Error(`Android AVD "${scenario.android.avdName}" is unavailable; installed AVDs: ${avds.join(', ') || '<none>'}`);
  }
  const simulator = await findIosSimulator();
  console.log(`✅ Local runtimes available: Android AVD ${scenario.android.avdName}; iOS ${simulator.name} (${simulator.state})`);
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
      ':composeApp:installDebug',
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
  const waitForMarker = (marker, timeoutMs = 300_000) => Promise.race([
    logcat.waitForMarker(marker, timeoutMs),
    result.then(
      () => {
        throw new Error(`Android UI test completed before expected E2E marker ${marker}`);
      },
      (error) => {
        throw new Error(`Android UI test failed before expected E2E marker ${marker}: ${error.message}`);
      },
    ),
  ]);
  return { ...logcat, result, waitForMarker };
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
  await page.bringToFront();
  console.log(`Web approving ${deviceName} join request`);
  await page.getByRole('link', { name: 'Devices', exact: true }).click();
  try {
    await page.getByTestId('pending-device-row').waitFor({ state: 'visible', timeout: 120_000 });
  } catch (error) {
    const bodyText = await page.locator('body').innerText().catch(() => '<unable to read Web page text>');
    console.error(`E2E Web device screen after waiting for ${deviceName} pending row:\n${bodyText}`);
    throw error;
  }
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

  let previousApprover;
  for (const cycle of recoveryCycles) {
    if (cycle.approver !== previousApprover) {
      console.log(`📱 Roles changed: Device A — Web requester; Device B — iOS ${cycle.approver === 'ios' ? 'approver' : 'observer'}; Device C — Android ${cycle.approver === 'android' ? 'approver' : 'observer'}`);
      previousApprover = cycle.approver;
    }
    console.log(`15.${cycle.number} Web requesting secret recovery (${cycle.approver} approves)`);
    await page.getByTestId(`secret-primary-action-${scenario.secret.name}`).click();
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

  }
}

async function main() {
  if (!existsSync(webDirectory)) throw new Error(`Web directory not found: ${webDirectory}`);
  if (!existsSync(composeRoot)) throw new Error(`Compose directory not found: ${composeRoot}`);
  mkdirSync(artifactsDirectory, { recursive: true });

  console.log(`\n=== ${scenario.name} ===`);
  await ensureIosFfi({ coreRoot, composeRoot, runAndWait, evidenceDirectory: artifactsDirectory });
  await preflightLocalRuntimes();
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

  const browser = await chromium.launch({ headless: false });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  page.on('console', (message) => console.log(`WEB CONSOLE [${message.type()}] ${message.text()}`));
  page.on('pageerror', (error) => console.error(`WEB PAGE ERROR ${error.message}`));
  await setupVirtualAuthenticator(page);

  console.log('5a. Opening and authenticating Web before business actions');
  await page.goto(scenario.web.url, { waitUntil: 'networkidle' });
  await unlockWithPasskeyIfNeeded(page);

  // All participating clients must be ready before the first Vault mutation.
  // This also catches an ADB-offline installation failure before the Web/iOS
  // scenario has created data.
  console.log('5b. Preparing Android before the business scenario');
  const androidSerial = await prepareAndroidEmulator();
  await installAndVerifyAndroidE2eApps({
    serial: androidSerial,
    appBundleId: scenario.android.bundleId,
    composeRoot,
    runAndWait,
    runAndCapture,
  });
  console.log('5c. Preparing iOS app and XCTest runner before the business scenario');
  const simulatorUdid = await prepareIosSimulator();
  const iosReadiness = await prepareIosTestApps({
    simulatorUdid,
    projectPath: iosProjectPath,
    derivedDataPath: iosDerivedDataPath,
    appBundleId: scenario.ios.bundleId,
    testSelector: 'iosAppUITests/CaseOneJoinFromIosUITest/testJoinApprovedVaultAndShowSecret',
    cwd: resolve(composeRoot, 'iosApp'),
    runAndWait,
    runAndCapture,
  });
  console.log(`✅ iOS app and XCTest runner are installed (${iosReadiness.appBundleId})`);

  console.log('5. Creating Vault');
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
  await page.getByTestId(`secret-primary-action-${scenario.secret.name}`).click();
  await page.getByText(scenario.secret.value, { exact: true }).waitFor();
  console.log('✅ Test #1 Web part passed');
  console.log('7a. Closing Web secret');
  await closeWebSecret(page);

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
  await page.getByTestId(`secret-primary-action-${scenario.secret.name}`).click();
  await page.getByText(scenario.secret.value, { exact: true }).waitFor({ timeout: 120_000 });
  console.log('✅ Web show secret after iOS join passed');
  console.log('12. Closing Web secret');
  await closeWebSecret(page);

  const androidTest = await startAndroidJoinTest(androidSerial);
  await androidTest.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT');
  await iosTest.waitForMarker('E2E: IOS_ANDROID_JOIN_NOTIFICATION_SUCCESS', 120_000);
  await approveJoinRequestOnWeb(page, 'Android');
  await androidTest.waitForMarker('E2E: ANDROID_SECRET_VISIBLE');

  await runRecoveryCycles(page, iosTest, androidTest);
  await Promise.all([iosTest.result, androidTest.result]);

  console.log('✅ Test #1 Web + iOS + Android part passed');
  if (exitOnSuccess) {
    await browser.close();
    await stopProcesses();
    return;
  }
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
