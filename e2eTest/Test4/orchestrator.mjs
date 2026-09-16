import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import process from 'node:process';
import { setTimeout as wait } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = dirname(fileURLToPath(import.meta.url));
const e2eRoot = resolve(root, '..');
const artifactsDirectory = resolve(e2eRoot, '.artifacts');
const scenarioArgument = process.argv.slice(2).find((argument) => !argument.startsWith('--'));
const exitOnSuccess = process.argv.includes('--exit-on-success') || process.env.E2E_EXIT_ON_SUCCESS === '1';
const scenarioPath = resolve(root, scenarioArgument ?? 'scenario.json');
const scenario = JSON.parse(readFileSync(scenarioPath, 'utf8'));
const diagnosticFileName = scenario.runId ? `test-4-${scenario.runId}.log` : 'test-4.log';
const diagnosticLogPath = resolve(artifactsDirectory, diagnosticFileName);
const recoveryCycles = (scenario.recovery.cyclePlan
  ? scenario.recovery.cyclePlan.flatMap((group) => Array.from({ length: group.count }, (_, offset) => {
      const resolve = (value) => Array.isArray(value) ? value[offset % value.length] : value;
      return {
        block: group.block,
        senders: group.senders,
        firstApprover: resolve(group.firstApprover),
        secondApprover: resolve(group.secondApprover),
      };
    }))
  : scenario.recovery.groups.flatMap((group) => {
  if (group.senders) {
    return Array.from({ length: group.count }, () => ({
      group: group.name,
      senders: group.senders,
      approvals: group.approvals,
      followUpApprovals: group.followUpApprovals ?? [],
    }));
  }
  const alternatingApprovers = group.approver.startsWith('alternate-')
    ? group.approver.split('-').slice(1)
    : null;
  return Array.from({ length: group.count }, (_, offset) => ({
    group: group.approver,
    approver: alternatingApprovers
      ? alternatingApprovers[offset % alternatingApprovers.length]
      : group.approver,
  }));
})).map((cycle, index) => ({ ...cycle, number: index + 1 }));
const recoveryShowTimeoutMs = scenario.recovery.showTimeoutMs ?? 45_000;
const cyclePlanJson = JSON.stringify(recoveryCycles);
const iosSenderCycles = recoveryCycles
  .filter((cycle) => cycle.senders.includes('ios'))
  .map((cycle) => cycle.number)
  .join(',');
const iosApprovalSteps = recoveryCycles
  .flatMap((cycle) => [[1, cycle.firstApprover], [2, cycle.secondApprover]]
    .filter(([, approver]) => approver === 'ios')
    .map(([step]) => `${cycle.number}:${step}`))
  .join(',');
const projectRoot = resolve(e2eRoot, '..');
const coreRoot = projectRoot;
const webDirectory = resolve(e2eRoot, scenario.web.directory);
const composeRoot = resolve(e2eRoot, scenario.ios.composeRoot);
const iosProjectPath = resolve(composeRoot, 'iosApp/iosApp.xcodeproj');
const iosDerivedDataPath = resolve(e2eRoot, '.derivedData/iosApp');
const androidEmulatorPath = '/Users/dmitrykuklin/Library/Android/sdk/emulator/emulator';
const androidTestBundleId = `${scenario.android.bundleId}.test`;
const iosTestMethod = scenario.ios.testMethod
  ?? 'testJoinAndroidInitiatedVaultAndHandleConcurrentRecovery';
const serverContainer = scenario.server.container;
const serverImage = scenario.server.image;
const processes = [];
const watchedProcessOutputs = [];
const browserDiagnostics = [];
let approvalCoordinator;
let currentCycleForDiagnostics = null;
let browserInstance;
let simulatorUdidForCleanup;
let androidSerialForCleanup;
let cleanupStarted = false;
const webSenderRevealCompletedCycles = new Set();

// Test #4 intentionally exercises many recovery claims. Keeping every byte of
// server/UI output in live JavaScript strings makes memory usage grow with
// every cycle and eventually crashes the Web target. Markers are short, so a
// bounded tail is sufficient for synchronization and failure diagnostics.
const maxWatchedOutputChars = 128_000;
const maxBrowserDiagnosticEntries = 200;
const maxBrowserDiagnosticLineChars = 8_000;

function appendBounded(previous, next, limit = maxWatchedOutputChars) {
  const combined = `${previous}${next}`;
  return combined.length > limit ? combined.slice(-limit) : combined;
}

function filterVerboseWatchedOutput(command, sink, text) {
  // The local server defaults to DEBUG and prints the complete claim map on
  // each state read. Keep useful info/warn/error lines without creating a
  // multi-hundred-megabyte diagnostic file and terminal stream.
  if (command === 'docker' && sink === 'stdout') {
    const lines = text.split(/\r?\n/).filter((line) => !line.includes(' DEBUG '));
    const filtered = lines.join('\n');
    return text.endsWith('\n') ? `${filtered}\n` : filtered;
  }
  return text;
}

function recordBrowserDiagnostic(line, { important = true } = {}) {
  if (!important) return;
  const boundedLine = line.length > maxBrowserDiagnosticLineChars
    ? `${line.slice(0, maxBrowserDiagnosticLineChars)}… [truncated]`
    : line;
  browserDiagnostics.push(boundedLine);
  if (browserDiagnostics.length > maxBrowserDiagnosticEntries) {
    browserDiagnostics.splice(0, browserDiagnostics.length - maxBrowserDiagnosticEntries);
  }
  appendFileSync(diagnosticLogPath, `[browser] ${boundedLine}\n`);
}

function writeDiagnostic(message) {
  const line = `[${new Date().toISOString()}] ${message}\n`;
  process.stderr.write(line);
  appendFileSync(diagnosticLogPath, line);
}

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
  let stdoutTail = '';
  let stderrTail = '';
  let markerScanTail = '';
  const markerWaiters = new Set();

  const child = run(command, args, { ...options, stdio: ['ignore', 'pipe', 'pipe'] });
  const processOutput = (chunk, sink) => {
    const text = chunk.toString();
    if (sink === 'stdout') stdoutTail = appendBounded(stdoutTail, text);
    else stderrTail = appendBounded(stderrTail, text);
    markerScanTail = appendBounded(markerScanTail, text);
    const diagnosticText = filterVerboseWatchedOutput(command, sink, text);
    if (diagnosticText) {
      process[sink].write(diagnosticText);
      appendFileSync(diagnosticLogPath, `[${command} ${sink}] ${diagnosticText}`);
    }
    for (const waiter of markerWaiters) {
      if (markerScanTail.includes(waiter.marker)) {
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
      if (code === 0) resolvePromise({ stdout: stdoutTail, stderr: stderrTail });
      else reject(processError);
    });
  });

  const waitForMarker = (marker, timeoutMs = 300_000) => new Promise((resolvePromise, reject) => {
    if (markerScanTail.includes(marker)) return resolvePromise();
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
    outputTail: () => `${stdoutTail}\n${stderrTail}`.slice(-12_000),
  };
  watchedProcessOutputs.push(watched);
  return watched;
}

function printFailureDiagnostics() {
  for (const watched of watchedProcessOutputs) {
    const tail = watched.outputTail();
    if (tail.trim()) {
      console.error(`\n=== E2E diagnostic: ${watched.command} ${watched.args.join(' ')} ===\n${tail}`);
      appendFileSync(diagnosticLogPath, `\n=== E2E diagnostic: ${watched.command} ${watched.args.join(' ')} ===\n${tail}\n`);
    }
  }
  if (browserDiagnostics.length > 0) {
    console.error(`\n=== E2E diagnostic: Web browser ===\n${browserDiagnostics.slice(-200).join('\n')}`);
    appendFileSync(diagnosticLogPath, `\n=== E2E diagnostic: Web browser ===\n${browserDiagnostics.slice(-200).join('\n')}\n`);
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
  if (cleanupStarted) return;
  cleanupStarted = true;
  await approvalCoordinator?.close().catch(() => {});
  approvalCoordinator = undefined;
  await browserInstance?.close().catch(() => {});
  browserInstance = undefined;
  for (const child of processes.reverse()) {
    if (!child.killed) child.kill('SIGTERM');
  }
  await wait(500);
  if (exitOnSuccess && androidSerialForCleanup) {
    await runAndWait(
      'adb',
      ['-s', androidSerialForCleanup, 'emu', 'kill'],
      { stdio: 'ignore' },
    ).catch(() => {});
  }
  if (exitOnSuccess && simulatorUdidForCleanup) {
    await runAndWait(
      'xcrun',
      ['simctl', 'shutdown', simulatorUdidForCleanup],
      { stdio: 'ignore' },
    ).catch(() => {});
  }
  if (exitOnSuccess) {
    await runAndWait('./gradlew', ['--stop'], {
      cwd: composeRoot,
      stdio: 'ignore',
    }).catch(() => {});
  }
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
  simulatorUdidForCleanup = simulator.udid;
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
      `-only-testing:iosAppUITests/CaseFourIosConcurrentRecoveryUITest/${iosTestMethod}`,
    ],
    {
      cwd: resolve(composeRoot, 'iosApp'),
      env: {
        E2E_VAULT_NAME: scenario.vault.name,
        E2E_SECRET_NAME: scenario.secret.name,
        E2E_SECRET_VALUE: scenario.secret.value,
        E2E_RECOVERY_CYCLES: String(recoveryCycles.length),
        // Keep iOS launch variables compact. XCTest's launch environment
        // truncates the full 18-cycle JSON payload before the test starts.
        E2E_IOS_SENDER_CYCLES: iosSenderCycles,
        E2E_IOS_APPROVAL_STEPS: iosApprovalSteps,
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
    // A stale named snapshot can report boot completed while leaving the
    // emulated shared-storage volume unmounted. Start from the current AVD
    // userdata without loading that snapshot; readiness is checked below.
    run(androidEmulatorPath, ['-avd', scenario.android.avdName, '-no-snapshot-load']);
    // `adb wait-for-device` also returns for an offline emulator. Wait for
    // the transport to become usable before starting Gradle.
    serial = await waitForOnlineAndroidDevice();
  }

  if (!serial) throw new Error(`Android emulator ${scenario.android.avdName} did not start`);
  androidSerialForCleanup = serial;
  console.log(`13. Preparing Android emulator: ${serial}`);
  await waitForAndroidBoot(serial);
  // sys.boot_completed can be published before the adb transport finishes
  // reconnecting after a snapshot restore. Verify the serial is online again
  // immediately before sending unlock input, otherwise adb may report the
  // device as not found/offline even though Android already reports booted.
  await waitForOnlineAndroidDevice(serial);
  await unlockAndroidDevice(serial);
  await waitForAndroidStorage(serial);
  await runAndWait('adb', ['-s', serial, 'uninstall', scenario.android.bundleId], { stdio: 'ignore' }).catch(() => {});
  await runAndWait('adb', ['-s', serial, 'uninstall', androidTestBundleId], { stdio: 'ignore' }).catch(() => {});
  return serial;
}

async function unlockAndroidDevice(serial) {
  console.log(`13a. Unlocking Android emulator: ${serial}`);
  await runAdbWithRetry(['-s', serial, 'shell', 'input', 'keyevent', 'KEYCODE_WAKEUP']);
  await runAdbWithRetry(['-s', serial, 'shell', 'input', 'swipe', '540', '1800', '540', '500', '300']);
  await runAdbWithRetry(['-s', serial, 'shell', 'input', 'text', '1111']);
  await runAdbWithRetry(['-s', serial, 'shell', 'input', 'keyevent', 'KEYCODE_ENTER']);
}

async function waitForOnlineAndroidDevice(expectedSerial = null, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastState = 'no emulator listed';
  while (Date.now() < deadline) {
    try {
      const { stdout } = await runAndCapture('adb', ['devices']);
      const rows = stdout.split('\n').map((line) => line.trim().split(/\s+/));
      const online = rows.find(([id, state]) =>
        id?.startsWith('emulator-') && state === 'device' &&
        (expectedSerial == null || id === expectedSerial));
      if (online?.[0]) return online[0];
      const emulator = rows.find(([id]) =>
        id?.startsWith('emulator-') &&
        (expectedSerial == null || id === expectedSerial));
      lastState = emulator ? `${emulator[0]} ${emulator[1] || '<empty>'}` : 'no emulator listed';
    } catch (error) {
      lastState = error.message;
    }
    await wait(1_000);
  }
  throw new Error(`Android emulator did not become online: ${lastState}`);
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

async function waitForAndroidStorage(serial, timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = 'Android shared storage is not mounted';
  let lastMountState = '<unknown>';

  while (Date.now() < deadline) {
    try {
      await runAndCapture('adb', ['-s', serial, 'shell', 'test', '-d', '/storage/self/primary']);
      // The Android emulator does not always pre-create this directory after
      // a clean boot. Creating the same parent that UTP will use verifies that
      // the FUSE-backed shared-storage mount is writable and makes the later
      // instrumentation mkdir deterministic.
      await runAndCapture(
        'adb',
        ['-s', serial, 'shell', 'mkdir', '-p', '/sdcard/Android/media'],
      );
      await runAndCapture('adb', ['-s', serial, 'shell', 'test', '-d', '/sdcard/Android/media']);
      console.log(`Android shared storage is ready: ${serial}`);
      return;
    } catch (error) {
      lastError = error.message;
      try {
        const { stdout } = await runAndCapture(
          'adb',
          [
            '-s', serial, 'shell', 'ls', '-ld',
            '/sdcard', '/storage/self', '/storage/self/primary', '/storage/emulated/0',
          ],
        );
        lastMountState = stdout.trim().replace(/\s+/g, ' ');
      } catch (mountError) {
        lastMountState = mountError.message;
      }
    }
    await wait(1_000);
  }

  throw new Error(`Android shared storage did not become ready: ${lastError}; mount=${lastMountState}`);
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
  // An emulator can briefly switch from `device` to `offline` after boot
  // while its services settle. AGP fails immediately in that state, so make
  // the state check immediately before handing control to Gradle.
  await waitForAndroidBoot(serial);
  // connectedDebugAndroidTest installs both the app and its instrumentation
  // APK, then launches the target activity itself. A separate `am start`
  // preflight is redundant and has proved flaky on the Android 16 emulator.
  await runAdbWithRetry(['-s', serial, 'logcat', '-c']);
  // The orchestrator uninstalls the app before each run, while Gradle may
  // consider installDebug up-to-date and skip reinstalling it. Install the
  // freshly assembled target APK explicitly so ActivityScenario can resolve
  // MainActivity even when the Gradle install task is cached.
  const appApkPath = resolve(composeRoot, 'composeApp/build/outputs/apk/debug/composeApp-debug.apk');
  await runAdbWithRetry(['-s', serial, 'install', '-r', appApkPath]);
  const logcat = watchProcessOutput('adb', ['-s', serial, 'logcat', 'MetaSecretE2E:I', '*:S']);
  void logcat.result.catch(() => {}); // logcat is stopped deliberately when the instrumentation test ends
  const test = runAndWait(
    './gradlew',
    [
      ':composeApp:installDebug',
      ':composeApp:connectedDebugAndroidTest',
      '-Pandroid.testInstrumentationRunnerArguments.class=metasecret.project.com.CaseFourAndroidConcurrentRecoveryTest',
      `-Pandroid.testInstrumentationRunnerArguments.vaultName=${scenario.vault.name}`,
      `-Pandroid.testInstrumentationRunnerArguments.secretName=${scenario.secret.name}`,
      `-Pandroid.testInstrumentationRunnerArguments.recoveryCycles=${recoveryCycles.length}`,
      `-Pandroid.testInstrumentationRunnerArguments.cyclePlan=${cyclePlanJson}`,
      '-Pandroid.testInstrumentationRunnerArguments.approvalCoordinatorUrl=http://10.0.2.2:5180',
    ],
    { cwd: composeRoot, env: { ANDROID_SERIAL: serial } },
  );
  const result = test.finally(() => logcat.child.kill('SIGTERM'));
  // The orchestrator waits for E2E markers before awaiting the full Android
  // result. Attach a rejection handler now so a parallel failure cannot turn
  // into an unhandled rejection and hide the original orchestration error.
  void result.catch(() => {});
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

async function waitForWebSecretValue(page) {
  await page.getByText(scenario.secret.value, { exact: true })
    .waitFor({ state: 'visible', timeout: recoveryShowTimeoutMs });
}

async function waitForWebRecoveryBadgeCount(page, count) {
  const badge = page.getByTestId('recovery-request-badge');
  await badge.filter({ hasText: String(count) }).waitFor({ state: 'visible', timeout: 120_000 });
  console.log(`[UI][Web] recovery badge count=${count}`);
}

async function waitForWebIncomingRecoveryCount(page, count) {
  const badge = page.getByTestId('recovery-request-badge');
  try {
    await badge.filter({ hasText: String(count) }).waitFor({ state: 'attached', timeout: 120_000 });
  } catch (error) {
    const badgeTexts = await badge.allTextContents().catch(() => []);
    const requestText = await page
      .getByTestId(`open-recovery-request-${scenario.secret.name}`)
      .textContent()
      .catch(() => null);
    writeDiagnostic(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: expected incoming badge ${count}; `
      + `visibleBadges=${JSON.stringify(badgeTexts).slice(0, 1_000)} `
      + `openRequestText=${JSON.stringify(requestText).slice(0, 500)}`,
    );
    throw error;
  }
  console.log(`[UI][Web] incoming recovery claim count=${count}`);
}

async function startWebRecovery(page) {
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: starting recovery request`);
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();
  await page.getByTestId(`secret-primary-action-${scenario.secret.name}`).click();
  await page.locator('[data-slot="dialog-content"]').waitFor({ state: 'visible', timeout: 30_000 });
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: recovery waiting dialog opened`);
}

async function dismissWebRecoveryWaitingUi(page, { senderClaimMayAlreadyBeAccepted = false } = {}) {
  if (senderClaimMayAlreadyBeAccepted) {
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: waiting for sender recovery `
      + 'to reveal after the first approval',
    );
    await page.getByText(scenario.secret.value, { exact: true }).waitFor({
      state: 'visible',
      timeout: recoveryShowTimeoutMs,
    });
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: sender recovery auto-revealed; `
      + 'closing it before incoming approval',
    );
    await closeWebSecret(page);
    webSenderRevealCompletedCycles.add(currentCycleForDiagnostics);
    return;
  }

  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: closing sender waiting dialog`);
  const close = page.locator('[data-slot="dialog-content"] [data-slot="dialog-close"]');
  if (!await close.isVisible().catch(() => false)) {
    console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: sender waiting dialog already closed`);
    return;
  }
  await close.click();
  await page.locator('[data-slot="dialog-content"]').waitFor({ state: 'hidden', timeout: 30_000 });
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: sender waiting dialog closed`);
}

async function approveIncomingRecoveryOnWeb(
  page,
  expectedCount = null,
  { senderClaimMayAlreadyBeAccepted = false } = {},
) {
  await dismissWebRecoveryWaitingUi(page, { senderClaimMayAlreadyBeAccepted });
  if (expectedCount != null) {
    await waitForWebRecoveryBadgeCount(page, expectedCount);
  }
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: waiting for incoming recovery marker`);
  const openRequest = page.getByTestId(`open-recovery-request-${scenario.secret.name}`);
  await openRequest.waitFor({ state: 'visible', timeout: 120_000 });
  await openRequest.click();
  await page.getByRole('button', { name: 'Approve', exact: true }).click();
  console.log(`[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: incoming recovery approved`);
}

async function revealAndCloseWebSecret(page, reopenClaim) {
  const primaryAction = page.getByTestId(`secret-primary-action-${scenario.secret.name}`);
  if (webSenderRevealCompletedCycles.delete(currentCycleForDiagnostics)) {
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: sender secret was already `
      + 'revealed after the first approval',
    );
    return;
  }
  if (reopenClaim) {
    console.log(`[UI][Web] reopening secret for reveal: ${scenario.secret.name}`);
    const actionBeforeWait = await primaryAction.textContent().catch(() => null);
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: waiting for primary action Show `
      + `(current=${JSON.stringify(actionBeforeWait?.trim() ?? null)})`,
    );
    // State invalidation is asynchronous after another device accepts the
    // sender's claim. Do not click while the button still says Recover: that
    // would create a second, unapproved recovery claim for the same cycle.
    await primaryAction.filter({ hasText: /^\s*Show\s*$/ }).waitFor({
      state: 'visible',
      timeout: 120_000,
    });
    const actionAfterWait = await primaryAction.textContent().catch(() => null);
    console.log(
      `[UI][Web] cycle ${currentCycleForDiagnostics ?? '?'}: primary action ready `
      + `(current=${JSON.stringify(actionAfterWait?.trim() ?? null)})`,
    );
    await primaryAction.click();
  } else {
    console.log(`[UI][Web] waiting for the already-open sender dialog to reveal ${scenario.secret.name}`);
  }
  await waitForWebSecretValue(page);
  console.log('[UI][Web] secret revealed; closing reveal dialog');
  await closeWebSecret(page);
  console.log('[UI][Web] reveal dialog closed');
}

async function runConcurrentRecoveryCycles(page, iosTest, androidTest) {
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();

  for (const cycle of recoveryCycles) {
    currentCycleForDiagnostics = cycle.number;
    console.log(`15.${cycle.number} ${cycle.senders.join(' + ')} request recovery concurrently; ${cycle.firstApprover} approves first`);

    // These two actions intentionally overlap. Each creates a different
    // sender-owned claim for the same secret, and neither request may consume
    // the other sender's response channel.
    const senderWaits = [];
    if (cycle.senders.includes('web')) {
      approvalCoordinator.allow('web-sender', cycle.number);
      senderWaits.push(startWebRecovery(page));
    }
    if (cycle.senders.includes('android')) {
      approvalCoordinator.allow('android-sender', cycle.number);
      senderWaits.push(androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_REQUEST_SENT_${cycle.number}`));
    }
    if (cycle.senders.includes('ios')) {
      approvalCoordinator.allow('ios-sender', cycle.number);
      senderWaits.push(iosTest.waitForMarker(`E2E: IOS_RECOVERY_REQUEST_SENT_${cycle.number}`, 180_000));
    }
    await Promise.all(senderWaits);
    // Sender-side markers mean that the native dialog was opened; they do not
    // by themselves prove that the recovery claim has reached the shared
    // state. Wait for every non-Web sender's claim to be visible on Web before
    // allowing any approver to act. This also covers the `web + ios` cycles,
    // where Android is the approver and otherwise could consume Web's claim
    // before iOS's claim had propagated to the receiver UI.
    const incomingClaimCount = cycle.senders.filter((sender) => sender !== 'web').length;
    if (incomingClaimCount > 0) {
      await waitForWebIncomingRecoveryCount(page, incomingClaimCount);
    }
    for (const [step, approver] of [[1, cycle.firstApprover], [2, cycle.secondApprover]]) {
      if (approver === 'web') {
        // The Web receiver must expose the number of still-actionable claims
        // for this secret. In cycles 10–12 this is 2 before the first click.
        const priorWebApprovals = [cycle.firstApprover, cycle.secondApprover]
          .slice(0, step - 1)
          .filter((previousApprover) => previousApprover === 'web').length;
        const incomingClaimCount = cycle.senders.filter((sender) => sender !== 'web').length - priorWebApprovals;
        await approveIncomingRecoveryOnWeb(page, incomingClaimCount, {
          senderClaimMayAlreadyBeAccepted: cycle.senders.includes('web') && cycle.firstApprover !== 'web',
        });
        console.log(`[UI][Web] cycle ${cycle.number}: approval ${step} complete`);
      } else {
        approvalCoordinator.allow(`${approver}-approve-${step}`, cycle.number);
        const test = approver === 'ios' ? iosTest : androidTest;
        await test.waitForMarker(`E2E: ${approver.toUpperCase()}_APPROVED_INCOMING_${cycle.number}_${step}`, 180_000);
      }
    }

    for (const sender of cycle.senders) {
      if (sender === 'web') {
        await revealAndCloseWebSecret(
          page,
          [cycle.firstApprover, cycle.secondApprover].includes('web'),
        );
      } else {
        approvalCoordinator.allow(`${sender}-show`, cycle.number);
        const test = sender === 'ios' ? iosTest : androidTest;
        await test.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_SECRET_VISIBLE_${cycle.number}`, recoveryShowTimeoutMs);
        await test.waitForMarker(`E2E: ${sender.toUpperCase()}_RECOVERY_CLOSED_${cycle.number}`, 60_000);
      }
    }
    console.log(`✅ Concurrent recovery ${cycle.number}/${recoveryCycles.length} passed`);

  }
}

async function main() {
  mkdirSync(artifactsDirectory, { recursive: true });
  writeFileSync(diagnosticLogPath, `=== Test #4 started ${new Date().toISOString()} ===\n`);
  writeDiagnostic(`scenario=${scenario.name}; cycles=${recoveryCycles.length}`);
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
  const serverLogLevel = process.env.E2E_SERVER_LOG_LEVEL ?? 'info';
  const server = watchProcessOutput(
    'docker',
    [
      'run', '--rm', '--name', serverContainer,
      '-e', `RUST_LOG=${serverLogLevel}`,
      '-p', `${scenario.server.port}:3000`, serverImage,
    ],
  );
  void server.result.catch(() => {});
  await waitForHttp(scenario.server.url);

  console.log('5. Starting Web in a visible browser');
  run('npm', ['run', 'dev', '--', '--host', 'localhost', '--port', '5173'], { cwd: webDirectory });
  await waitForHttp(scenario.web.url);

  browserInstance = await chromium.launch({ headless: false });
  const browser = browserInstance;
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  // The WASM tracing layer writes every DEBUG state snapshot to
  // console.debug. With repeated recovery cycles those snapshots become very
  // large and can
  // make the browser target retain hundreds of megabytes of inspector data.
  // E2E markers and failure diagnostics are emitted by the native runners and
  // orchestrator, so suppress only browser DEBUG output for this test page.
  await page.addInitScript(() => {
    console.debug = () => {};
  });
  page.on('console', (message) => {
    const line = `[${message.type()}] ${message.text()}`;
    recordBrowserDiagnostic(line, { important: message.type() !== 'debug' });
  });
  page.on('pageerror', (error) => {
    const line = `[pageerror] ${error.message}`;
    recordBrowserDiagnostic(line);
  });
  await setupVirtualAuthenticator(page);

  // Clean the iOS simulator before Android starts. Test #4's Android helper
  // approves the first pending join request, so a left-over iOS UI-test app
  // from an interrupted run could otherwise be mistaken for the Web request.
  // Test #3 is safe without this because iOS is launched only after Web has
  // already been approved.
  const simulatorUdid = await prepareIosSimulator();

  const androidSerial = await prepareAndroidEmulator();
  const androidTest = await startAndroidJoinTest(androidSerial);
  await androidTest.waitForMarker('E2E: ANDROID_INITIATOR_READY');

  console.log('Web joining Android-created vault');
  await page.goto(scenario.web.url, { waitUntil: 'networkidle' });
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').fill(scenario.vault.name);
  await page.getByRole('button', { name: 'Set Vault Name' }).click();
  await page.getByRole('button', { name: 'Join', exact: true }).click();
  await androidTest.waitForMarker('E2E: ANDROID_WEB_JOIN_APPROVED', 120_000);
  await page.getByRole('button', { name: '+ Add Secret' }).waitFor({ timeout: 120_000 });

  const iosTest = startIosJoinTest(simulatorUdid);
  // Same as Android above: this test runs in parallel with the orchestrator.
  // Keep its eventual rejection observable through `iosTest.result`, while
  // preventing Node from reporting it as an unhandled rejection during
  // cleanup.
  void iosTest.result.catch(() => {});
  await iosTest.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT');
  approvalCoordinator.allow('android-ios-join', 0);
  await androidTest.waitForMarker('E2E: ANDROID_IOS_JOIN_APPROVED', 120_000);
  // Let iOS finish its own join timeout (and emit a useful failure) before
  // stopping the parallel Android instrumentation.
  await iosTest.waitForMarker('E2E: IOS_SECRET_VISIBLE', 210_000);

  await runConcurrentRecoveryCycles(page, iosTest, androidTest);
  await Promise.all([iosTest.result, androidTest.result]);

  console.log('✅ Test #4 concurrent Web + iOS + Android recovery passed');
  if (exitOnSuccess) {
    await stopProcesses();
    process.exit(0);
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
  console.error(`\n❌ Test #4 failed: ${error.message}`);
  writeDiagnostic(`FAILED: ${error.stack ?? error.message}`);
  printFailureDiagnostics();
  await stopProcesses();
  process.exit(1);
});
