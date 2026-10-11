import { ensureIosFfi, prepareIosXcodeBuild, recordIosXcodeBuild } from '../shared/ios-ffi-provenance.mjs';
import { appendFileSync, createWriteStream, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { once as onceEvent } from 'node:events';
import process from 'node:process';
import { setTimeout as wait } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { discoverAndroidSdk, findOnlineAndroidAvd, withAndroidSdkPath } from '../shared/android-sdk.mjs';
import { installAndVerifyAndroidE2eApps, installBuiltIosTestApps } from '../shared/platform-readiness.mjs';
import { openVisibleIosDeviceUi } from '../shared/visible-ios-ui.mjs';

const root = dirname(fileURLToPath(import.meta.url));
const e2eRoot = resolve(root, '..');
const runTimestamp = process.env.E2E_RUN_ID
  ? `${process.env.E2E_RUN_ID}-attempt-${process.env.E2E_TEST_ATTEMPT ?? '1'}`
  : new Date().toISOString().replace(/[:.]/g, '-');
const artifactsDirectory = resolve(e2eRoot, '.artifacts');
mkdirSync(artifactsDirectory, { recursive: true });
const runLogPath = resolve(artifactsDirectory, `test-2-run-${runTimestamp}.log`);
const runLogStream = createWriteStream(runLogPath, { flags: 'wx' });
const runLogPending = { stdout: '', stderr: '' };
let runLogClosed = false;
let runLogClosing = false;
let runLogError;
let redactPemContent = false;
const runLogPostFlush = [];
runLogStream.on('error', (error) => { runLogError = error; });

function redactRunLogLine(line) {
  if (redactPemContent) {
    if (/-----END [^-]+-----/.test(line)) redactPemContent = false;
    return '[REDACTED_PEM_CONTENT]\n';
  }
  let redacted = line;
  const pemStart = redacted.match(/-----BEGIN ([^-]+)-----/);
  if (pemStart) {
    redactPemContent = !redacted.includes(`-----END ${pemStart[1]}-----`);
    return `[REDACTED_PEM_${pemStart[1].replace(/\s+/g, '_')}]\n`;
  }
  const testSecret = scenario?.secret?.value;
  if (typeof testSecret === 'string' && testSecret.length > 0) {
    redacted = redacted.split(testSecret).join('[REDACTED_SECRET]');
  }
  redacted = redacted
    .replace(/(["']?(?:secret(?:[_\s-]?value)?|master[\s_-]?key|private[\s_-]?key|key[\s_-]?share|recovery[\s_-]?key)["']?\s*[:=]\s*)(?:"[^"]*"|'[^']*'|[^\s,;}\]]+)/gi, '$1[REDACTED]')
    .replace(/\b[A-Za-z0-9+/]{96,}={0,2}\b/g, '[REDACTED_BASE64]')
    .replace(/\b[0-9a-f]{96,}\b/gi, '[REDACTED_HEX_KEY]');
  return redacted;
}

function appendRunLog(streamName, chunk) {
  if (runLogError) return;
  const text = Buffer.isBuffer(chunk) ? chunk.toString('utf8') : String(chunk);
  if (runLogClosing) {
    runLogPostFlush.push({ streamName, text });
    return;
  }
  if (runLogClosed) {
    appendFileSync(runLogPath, `[${new Date().toISOString()}][${streamName}] ${redactRunLogLine(text)}\n`);
    return;
  }
  const combined = `${runLogPending[streamName]}${text}`;
  const lines = combined.split(/(?<=\n)/);
  runLogPending[streamName] = '';
  if (lines.length > 0 && !lines[lines.length - 1].endsWith('\n')) {
    runLogPending[streamName] = lines.pop();
  }
  for (const line of lines) {
    runLogStream.write(`[${new Date().toISOString()}][${streamName}] ${redactRunLogLine(line)}`);
  }
}

for (const streamName of ['stdout', 'stderr']) {
  const stream = process[streamName];
  const originalWrite = stream.write.bind(stream);
  stream.write = function writeWithDurableRunLog(chunk, ...args) {
    appendRunLog(streamName, chunk);
    return originalWrite(chunk, ...args);
  };
}

async function flushAndCloseRunLog() {
  if (runLogClosed || runLogClosing) return;
  runLogClosing = true;
  for (const streamName of ['stdout', 'stderr']) {
    const pending = runLogPending[streamName];
    if (pending) runLogStream.write(`[${new Date().toISOString()}][${streamName}] ${redactRunLogLine(pending)}\n`);
    runLogPending[streamName] = '';
  }
  await new Promise((resolvePromise) => runLogStream.end(resolvePromise));
  runLogClosing = false;
  runLogClosed = true;
  for (const { streamName, text } of runLogPostFlush.splice(0)) {
    appendFileSync(runLogPath, `[${new Date().toISOString()}][${streamName}] ${redactRunLogLine(text)}\n`);
  }
  if (runLogError) console.error(`Could not finish durable run log ${runLogPath}: ${runLogError.message}`);
}
const scenarioArgument = process.argv.slice(2).find((argument) => !argument.startsWith('--'));
const scenarioPath = resolve(root, scenarioArgument ?? 'scenario.json');
const scenario = JSON.parse(readFileSync(scenarioPath, 'utf8'));
const exitOnSuccess = process.argv.includes('--exit-on-success') || process.env.E2E_EXIT_ON_SUCCESS === '1';
const recoveryCycles = scenario.recovery.groups.flatMap((group) => {
  const alternatingApprovers = group.approver.startsWith('alternate-')
    ? group.approver.split('-').slice(1)
    : null;
  return Array.from({ length: group.count }, (_, offset) => ({
    group: group.approver,
    approver: alternatingApprovers
      ? alternatingApprovers[offset % alternatingApprovers.length]
      : group.approver,
  }));
}).map((cycle, index) => ({ ...cycle, number: index + 1 }));
const recoveryShowTimeoutMs = scenario.recovery.showTimeoutMs ?? 45_000;
const iosRecoveryApprovalCycles = recoveryCycles.filter((cycle) => cycle.approver === 'ios').map((cycle) => cycle.number);
const androidRecoveryApprovalCycles = recoveryCycles.filter((cycle) => cycle.approver === 'android').map((cycle) => cycle.number);
const projectRoot = resolve(e2eRoot, '..');
const coreRoot = projectRoot;
const webDirectory = resolve(e2eRoot, scenario.web.directory);
const composeRoot = resolve(e2eRoot, scenario.ios.composeRoot);
const iosProjectPath = resolve(composeRoot, 'iosApp/iosApp.xcodeproj');
const iosDerivedDataPath = resolve(e2eRoot, '.derivedData/iosApp');
const iosResultBundlePath = resolve(artifactsDirectory, `test-2-run-${runTimestamp}.xcresult`);
const xcresultTestsPath = resolve(artifactsDirectory, `test-2-run-${runTimestamp}-xcresult-tests.json`);
const xcresultActivitiesPath = resolve(artifactsDirectory, `test-2-run-${runTimestamp}-xcresult-activities.json`);
const androidSdkTools = discoverAndroidSdk();
const androidEmulatorPath = androidSdkTools.emulatorPath;
const serverContainer = scenario.server.container;
const serverImage = scenario.server.image;
const processes = [];
const watchedProcessOutputs = [];
const browserDiagnostics = [];
let activeAndroidSerial;
let activeBrowserPage;
let androidRecoveryDiagnostics;
let activeIosSimulatorUdid;
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

async function waitForAndroidPendingRecovery(cycle, startCursor, timeoutMs = 45_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const pendingEvent = androidRecoveryDiagnostics?.pendingRecoveryEvents.get(cycle);
    if (pendingEvent && pendingEvent.sequence > startCursor) return;
    await wait(100);
  }
  const recent = (androidRecoveryDiagnostics?.recentStateEvents ?? []).slice(-12)
    .map(({ sequence, line }) => `state#${sequence} ${line}`);
  throw new Error(
    `Android did not report a targeted pending recovery claim for cycle ${cycle} within ${timeoutMs} ms. ` +
    `Recent Android state events: ${recent.join(' | ') || 'none'}`,
  );
}
function run(command, args, options = {}) {
  const stdio = options.stdio ?? ['ignore', 'pipe', 'pipe'];
  const child = spawn(command, args, {
    cwd: options.cwd ?? projectRoot,
    env: {
      ...process.env,
      PATH: withAndroidSdkPath(`/opt/homebrew/bin:/usr/local/bin:${process.env.PATH ?? ''}`, androidSdkTools),
      ...(options.env ?? {}),
    },
    stdio,
    shell: false,
  });
  for (const streamName of ['stdout', 'stderr']) {
    const childStream = child[streamName];
    if (!childStream) continue;
    childStream.on('data', (chunk) => {
      if (options.captureQuiet) appendRunLog(streamName, chunk);
      else process[streamName].write(chunk);
    });
  }
  processes.push(child);
  return child;
}

function runAndWait(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = run(command, args, { ...options, stdio: options.stdio ?? ['ignore', 'pipe', 'pipe'] });
    child.once('error', reject);
    child.once('close', (code, signal) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`${command} exited with ${code ?? signal}`));
    });
  });
}

function runAndCapture(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = run(command, args, { ...options, captureQuiet: options.captureQuiet ?? true, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString();
    });
    child.once('error', reject);
    child.once('close', (code, signal) => {
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
    child.once('close', (code, signal) => {
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

function normalizeAndroidRecoveryDiagnostic(line) {
  if (line.includes('E2E_DIAG recovery-invalidation received')) {
    return 'recovery-invalidation received';
  }
  const appState = line.match(/E2E_DIAG recovery-appstate success=(true|false) member=(true|false)/);
  if (appState) return `recovery-appstate success=${appState[1]} member=${appState[2]}`;

  const state = line.match(
    /E2E_DIAG recovery-state membership=(member|non-member) recoverClaims=(\d+) targeted=(\d+) pendingApproval=(\d+) done=(\d+)/,
  );
  if (state) {
    return `recovery-state membership=${state[1]} recoverClaims=${state[2]} targeted=${state[3]} ` +
      `pendingApproval=${state[4]} done=${state[5]}`;
  }

  const socket = line.match(/E2E_DIAG recovery-socket (connected)/);
  if (socket) return `recovery-socket ${socket[1]}`;
  const ui = line.match(/E2E: ANDROID_RECOVERY_BADGE_DIAGNOSTIC_(\d+) secretRowCount=(\d+) badgeCount=(\d+) openActionCount=(\d+)/);
  if (ui) {
    return `android-ui cycle=${ui[1]} secretRowCount=${ui[2]} badgeCount=${ui[3]} openActionCount=${ui[4]}`;
  }
  return null;
}

function startAndroidRecoveryDiagnosticLogcat(serial) {
  const child = run('adb', ['-s', serial, 'logcat', '-v', 'brief', '-s', 'MetaSecretE2E:I'], {
    stdio: ['ignore', 'pipe', 'ignore'],
  });
  const lines = [];
  const recentStateEvents = [];
  const pendingRecoveryEvents = new Map();
  let stateSequence = 0;
  let buffer = '';
  child.stdout.on('data', (chunk) => {
    buffer += chunk.toString();
    const pending = buffer.split(/\r?\n/);
    buffer = pending.pop() ?? '';
    for (const line of pending) {
      const safeLine = normalizeAndroidRecoveryDiagnostic(line);
      if (safeLine) {
        const diagnosticLine = `${new Date().toISOString()} ${safeLine}`;
        lines.push(diagnosticLine);
        const state = safeLine.match(
          /^recovery-state membership=member recoverClaims=(\d+) targeted=(\d+) pendingApproval=(\d+) done=(\d+)$/,
        );
        if (state) {
          const event = { sequence: ++stateSequence, line: diagnosticLine };
          recentStateEvents.push(event);
          if (recentStateEvents.length > 12) recentStateEvents.shift();
          if (Number(state[2]) > 0 && Number(state[3]) > 0) {
            pendingRecoveryEvents.set(Number(state[1]), event);
          }
        }
      }
    }
    if (lines.length > 200) lines.splice(0, lines.length - 200);
  });
  child.on('error', (error) => console.warn(`Android scoped diagnostic logcat stopped: ${error.message}`));
  return {
    child,
    lines,
    recentStateEvents,
    pendingRecoveryEvents,
    get stateSequence() { return stateSequence; },
  };
}

async function captureFailureArtifacts() {
  const artifactsDirectory = resolve(e2eRoot, '.artifacts');
  mkdirSync(artifactsDirectory, { recursive: true });

  if (activeBrowserPage) {
    const screenshot = resolve(artifactsDirectory, 'test-2-cycle-1-failure-web.png');
    await activeBrowserPage.screenshot({ path: screenshot }).catch((error) => {
      console.warn(`Could not capture Web failure screenshot: ${error.message}`);
    });
  }

  if (activeAndroidSerial) {
    const remoteScreenshot = '/sdcard/Download/meta-secret-test-2-failure.png';
    const screenshot = resolve(artifactsDirectory, 'test-2-cycle-1-failure-android.png');
    await runAndWait('adb', ['-s', activeAndroidSerial, 'shell', 'screencap', '-p', remoteScreenshot], {
      stdio: 'ignore',
    }).then(() => runAndWait('adb', ['-s', activeAndroidSerial, 'pull', remoteScreenshot, screenshot], {
      stdio: 'ignore',
    })).catch((error) => console.warn(`Could not capture Android failure screenshot: ${error.message}`));
    await runAndWait('adb', ['-s', activeAndroidSerial, 'shell', 'rm', '-f', remoteScreenshot], {
      stdio: 'ignore',
    }).catch(() => {});
  }

  if (androidRecoveryDiagnostics?.lines.length) {
    const stateEvents = androidRecoveryDiagnostics.recentStateEvents
      .map(({ sequence, line }) => `state#${sequence} ${line}`)
      .join('\n');
    writeFileSync(
      resolve(artifactsDirectory, 'test-2-android-recovery-diagnostics.log'),
      `${androidRecoveryDiagnostics.lines.join('\n')}\n\n` +
      `Monotonic Android state cursor: ${androidRecoveryDiagnostics.stateSequence}\n` +
      `Recent recovery states:\n${stateEvents}\n`,
    );
  }

  const androidMarkers = watchedProcessOutputs.find((watched) =>
    watched.command === 'adb' && watched.args.includes('MetaSecretE2E:I'));
  if (androidMarkers?.outputTail().trim()) {
    writeFileSync(
      resolve(artifactsDirectory, 'test-2-android-e2e-markers.log'),
      androidMarkers.outputTail(),
    );
  }
}

function printFailureDiagnostics() {
  for (const watched of watchedProcessOutputs) {
    const tail = watched.outputTail();
    if (tail.trim()) {
      console.error(`\n=== E2E diagnostic: ${watched.command} ${watched.args.join(' ')} ===\n${tail}`);
    }
  }
  if (browserDiagnostics.length > 0) {
    console.error(`\n=== E2E diagnostic: Web browser ===\n${browserDiagnostics.slice(-200).join('\n')}`);
  }
}

function collectTestIdentifiers(value, result = []) {
  if (!value || typeof value !== 'object') return result;
  if (Array.isArray(value)) {
    for (const child of value) collectTestIdentifiers(child, result);
    return result;
  }
  for (const [key, child] of Object.entries(value)) {
    if ((key === 'testIdentifier' || key === 'nodeIdentifier') && typeof child === 'string') {
      result.push(child);
    } else if (child && typeof child === 'object') {
      collectTestIdentifiers(child, result);
    }
  }
  return result;
}

async function collectXcresultDiagnostics() {
  writeFileSync(`${iosResultBundlePath}.path.txt`, `${iosResultBundlePath}\n`);
  console.log(`iOS result bundle path: ${iosResultBundlePath}`);
  if (!existsSync(iosResultBundlePath)) {
    console.error('xcresult bundle was not created before the iOS run stopped.');
    return;
  }

  const testsResult = await runAndCapture('xcrun', [
    'xcresulttool', 'get', 'test-results', 'tests', '--path', iosResultBundlePath,
  ]).catch((error) => ({ stdout: '', stderr: error.message }));
  const safeTestsOutput = redactRunLogLine(testsResult.stdout || testsResult.stderr || 'xcresulttool returned no test output');
  writeFileSync(xcresultTestsPath, `${safeTestsOutput}\n`);
  console.log(`\n=== xcresulttool test results ===\n${safeTestsOutput}`);

  let testIdentifiers = [];
  try {
    testIdentifiers = collectTestIdentifiers(JSON.parse(testsResult.stdout));
  } catch {
    // Keep the test-results output above; older or incomplete bundles may not
    // produce parseable JSON.
  }
  const testIdentifier = testIdentifiers.find((identifier) => identifier.includes('testCreateVaultThenApproveAndroidAndWeb'))
    ?? testIdentifiers.find((identifier) => identifier.includes('CaseTwoIosInitiatorUITest'));
  if (!testIdentifier) {
    console.error('Could not find the Test #2 identifier in xcresult test results; activity export skipped.');
    return;
  }

  const activitiesResult = await runAndCapture('xcrun', [
    'xcresulttool', 'get', 'test-results', 'activities',
    '--path', iosResultBundlePath,
    '--test-id', testIdentifier,
  ]).catch((error) => ({ stdout: '', stderr: error.message }));
  const safeActivitiesOutput = redactRunLogLine(
    activitiesResult.stdout || activitiesResult.stderr || 'xcresulttool returned no activity output',
  );
  writeFileSync(xcresultActivitiesPath, `${safeActivitiesOutput}\n`);
  console.log(`\n=== xcresulttool Test #2 activities (${testIdentifier}) ===\n${safeActivitiesOutput}`);
}

function dumpWatchedProcessTails() {
  for (const watched of watchedProcessOutputs) {
    const tail = watched.outputTail();
    if (tail.trim()) {
      console.error(`\n=== Captured tail before cleanup: ${watched.command} ${watched.args.join(' ')} ===\n${tail}`);
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
  const ownedProcesses = processes.slice().reverse();
  for (const child of ownedProcesses) {
    if (child.exitCode == null && child.signalCode == null && !child.killed) child.kill('SIGTERM');
  }
  const waitForClose = (child) => child.exitCode != null || child.signalCode != null
    ? Promise.resolve()
    : onceEvent(child, 'close').catch(() => {});
  await Promise.race([
    Promise.all(ownedProcesses.map(waitForClose)),
    wait(25_000),
  ]);
  const stillRunning = ownedProcesses.filter((child) => child.exitCode == null && child.signalCode == null);
  for (const child of stillRunning) child.kill('SIGKILL');
  await Promise.race([
    Promise.all(stillRunning.map(waitForClose)),
    wait(5_000),
  ]);

  // Only stop visible runtimes after the iOS XCTest and Android instrumentation
  // processes have exited. Shutting a simulator down first can cut off a
  // sibling platform that is still completing or reporting its failure.
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
  activeIosSimulatorUdid = simulator.udid;
  console.log(`8. Preparing iOS simulator: ${simulator.name} (${simulator.udid})`);
  if (simulator.state !== 'Booted') {
    await runAndWait('xcrun', ['simctl', 'boot', simulator.udid]);
  }
  await runAndWait('xcrun', ['simctl', 'bootstatus', simulator.udid, '-b']);
  if (process.env.E2E_RANGE_RUN !== '1') {
    await runAndWait('xcrun', ['simctl', 'uninstall', simulator.udid, scenario.ios.bundleId], { stdio: 'ignore' }).catch(() => {});
  }
  await openVisibleIosDeviceUi({ runAndCapture, runAndWait, simulatorUdid: simulator.udid });
  return simulator.udid;
}

async function startIosJoinTest(simulatorUdid) {
  const nativeInputs = await prepareIosXcodeBuild({ coreRoot, composeRoot, derivedDataPath: iosDerivedDataPath, runAndCapture });
  const xcodebuildArgs = [
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
    '-only-testing:iosAppUITests/CaseTwoIosInitiatorUITest/testCreateVaultThenApproveAndroidAndWeb',
  ];
  const xcodebuildOptions = {
    cwd: resolve(composeRoot, 'iosApp'),
    env: {
      E2E_VAULT_NAME: scenario.vault.name,
      E2E_SECRET_NAME: scenario.secret.name,
      E2E_SECRET_VALUE: scenario.secret.value,
      E2E_RECOVERY_CYCLES: String(recoveryCycles.length),
      E2E_IOS_RECOVERY_APPROVALS: iosRecoveryApprovalCycles.join(','),
      E2E_APPROVAL_COORDINATOR_URL: 'http://127.0.0.1:5180',
    },
  };

  console.log('9. Building iOS UI test once');
  return runAndWait('xcodebuild', ['build-for-testing', ...xcodebuildArgs], xcodebuildOptions)
    .then(async () => {
      recordIosXcodeBuild({ coreRoot, composeRoot, derivedDataPath: iosDerivedDataPath, inputs: nativeInputs });
      await installBuiltIosTestApps({
        simulatorUdid,
        derivedDataPath: iosDerivedDataPath,
        appBundleId: scenario.ios.bundleId,
        runAndWait,
        runAndCapture,
      });
      console.log('10. Starting iOS UI test without building');
      console.log(`iOS result bundle: ${iosResultBundlePath}`);
      return watchProcessOutput(
        'xcodebuild',
        ['test-without-building', ...xcodebuildArgs, '-resultBundlePath', iosResultBundlePath],
        xcodebuildOptions,
      );
    });
}

async function prepareAndroidEmulator() {
  let serial = await findOnlineAndroidAvd(androidSdkTools, scenario.android.avdName, runAndCapture);

  if (!serial) {
    console.log(`12. Starting Android emulator: ${scenario.android.avdName}`);
    run(androidEmulatorPath, ['-avd', scenario.android.avdName]);
    serial = await waitForConfiguredAndroidAvd();
  }

  if (!serial) throw new Error(`Android emulator ${scenario.android.avdName} did not start`);
  console.log(`13. Preparing Android emulator: ${serial}`);
  await waitForAndroidBoot(serial);
  if (process.env.E2E_RANGE_RUN !== '1') {
    await runAndWait('adb', ['-s', serial, 'uninstall', scenario.android.bundleId], { stdio: 'ignore' }).catch(() => {});
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
  androidRecoveryDiagnostics = startAndroidRecoveryDiagnosticLogcat(serial);
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
      '-Pandroid.testInstrumentationRunnerArguments.stateDrivenRecoveryWait=true',
    ],
    { cwd: composeRoot, env: { ANDROID_SERIAL: serial } },
  );
  const result = test.finally(() => logcat.child.kill('SIGTERM'));
  // If another participant fails first, cleanup terminates Gradle. Keep that
  // expected abort from becoming an unhandled rejection while the caller
  // reports the earliest failing gate.
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
  await page.bringToFront();
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

async function runRecoveryCyclesFromIos(page, iosTest, androidTest) {
  await page.getByRole('link', { name: 'Secrets', exact: true }).click();

  for (const cycle of recoveryCycles) {
    const androidRole = cycle.approver === 'android' ? 'approver' : 'observer';
    const webRole = cycle.approver === 'web' ? 'approver' : 'observer';
    console.log(`📱 Roles changed: Device A — Web ${webRole}; Device B — iOS requester; Device C — Android ${androidRole}`);
    console.log(`15.${cycle.number} iOS requesting secret recovery (${cycle.approver} approves)`);
    const androidStateEventStart = androidRecoveryDiagnostics?.stateSequence ?? 0;
    approvalCoordinator.allow('ios-sender', cycle.number);
    await iosTest.waitForMarker(`E2E: IOS_RECOVERY_REQUEST_SENT_${cycle.number}`);
    // An incoming recovery request changes this <li>'s ARIA role from listitem
    const openRequest = page.getByTestId(`open-recovery-request-${scenario.secret.name}`);
    await openRequest.waitFor({ state: 'visible', timeout: 120_000 });
    console.log(`Web received recovery ${cycle.number}; opening its recovery request`);
    await openRequest.click();

    console.log(`Waiting for Android app-state to contain a pending recovery claim for cycle ${cycle.number}`);
    await waitForAndroidPendingRecovery(cycle.number, androidStateEventStart);
    approvalCoordinator.allow('android-state', cycle.number);
    await androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_STATE_PENDING_${cycle.number}`);
    console.log(`✅ Android app-state received the pending recovery claim for cycle ${cycle.number}`);
    await androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_REQUEST_ALERT_${cycle.number}`);
    if (cycle.approver === 'web') {
      console.log(`Web approving recovery ${cycle.number}`);
      await page.getByRole('button', { name: 'Approve', exact: true }).click();
    } else {
      approvalCoordinator.allow('android', cycle.number);
      await androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_APPROVE_SUCCESS_${cycle.number}`);
    }

    await iosTest.waitForMarker(`E2E: IOS_RECOVERY_SECRET_VISIBLE_${cycle.number}`, recoveryShowTimeoutMs);
    console.log(`✅ Recovery ${cycle.number}/${recoveryCycles.length}: iOS show secret passed`);
    await iosTest.waitForMarker(`E2E: IOS_RECOVERY_CLOSED_${cycle.number}`);
    await openRequest.waitFor({ state: 'hidden', timeout: 120_000 }).catch(() => {});

  }
}

async function main() {
  if (!existsSync(webDirectory)) throw new Error(`Web directory not found: ${webDirectory}`);
  if (!existsSync(composeRoot)) throw new Error(`Compose directory not found: ${composeRoot}`);

  console.log(`Durable runner log: ${runLogPath}`);
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
  activeBrowserPage = page;
  page.on('console', (message) => {
    browserDiagnostics.push(`[${message.type()}] ${message.text()}`);
  });
  page.on('pageerror', (error) => {
    browserDiagnostics.push(`[pageerror] ${error.message}`);
  });
  await setupVirtualAuthenticator(page);
  console.log('5a. Opening the Web UI while native devices prepare');
  await page.goto(scenario.web.url, { waitUntil: 'networkidle' });

  // Finish Android compilation and installation before iOS enters the
  // business flow. Otherwise iOS waits for an Android join request while
  // Gradle is still building the instrumentation APK in parallel.
  const androidSerial = await prepareAndroidEmulator();
  activeAndroidSerial = androidSerial;
  console.log('14. Installing and verifying the Android app and instrumentation before the scenario');
  await installAndVerifyAndroidE2eApps({
    serial: androidSerial,
    appBundleId: scenario.android.bundleId,
    composeRoot,
    runAndWait,
    runAndCapture,
  });

  const simulatorUdid = await prepareIosSimulator();
  console.log('✅ Android is installed; iPhone 17 is booted and the visible Device Hub is open. Starting the business scenario now.');
  if (process.env.E2E_RANGE_RUN !== '1') {
    console.log('🚀 TEST #2 HAS BEEN STARTED!');
    console.log('📱 Devices: Device A — iOS (iPhone 17); Device B — Android (Pixel_4a); Device C — Web (visible Chrome)');
    console.log('📖 Scenario: Device A creates a Vault and Secret; Devices B and C join; Device A requests recovery across 18 cycles; Devices B and C approve in the declared order; Device A shows the recovered Secret.');
  }
  const iosTest = await startIosJoinTest(simulatorUdid);
  await Promise.race([
    iosTest.waitForMarker('E2E: IOS_INITIATOR_READY'),
    iosTest.result.then(() => {
      throw new Error('iOS UI test finished before join request marker');
    }),
  ]);

  const androidTest = await startAndroidJoinTest(androidSerial);
  await androidTest.waitForMarker('E2E: ANDROID_JOIN_REQUEST_SENT');
  await iosTest.waitForMarker('E2E: IOS_ANDROID_JOIN_APPROVED', 120_000);
  await androidTest.waitForMarker('E2E: ANDROID_SECRET_VISIBLE');

  console.log('Web joining iOS-created vault');
  await page.bringToFront();
  await unlockWithPasskeyIfNeeded(page);
  await page.getByPlaceholder('vault name').fill(scenario.vault.name);
  await page.getByRole('button', { name: 'Set Vault Name' }).click();
  await page.getByRole('button', { name: 'Join', exact: true }).click();
  await iosTest.waitForMarker('E2E: IOS_WEB_JOIN_APPROVED', 120_000);
  await page.getByText('Your request to join this vault is pending approval.', { exact: true })
    .waitFor({ state: 'hidden', timeout: 120_000 });
  await page.getByRole('button', { name: '+ Add Secret' }).waitFor({ timeout: 120_000 });

  await runRecoveryCyclesFromIos(page, iosTest, androidTest);
  await Promise.all([iosTest.result, androidTest.result]);

  console.log('✅ Test #2 Web + iOS + Android scenario passed');
  await collectXcresultDiagnostics();
  if (exitOnSuccess) {
    await browser.close();
    await flushAndCloseRunLog();
    await stopProcesses();
    console.log('🏁 🟢 TEST SUCCESS');
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
  console.error(`\n❌ Test #2 failed: ${error.message}`);
  dumpWatchedProcessTails();
  printFailureDiagnostics();
  await captureFailureArtifacts();
  console.error(`Durable runner log: ${runLogPath}`);
  console.error(`iOS result bundle (when xcodebuild produced it): ${iosResultBundlePath}`);
  await flushAndCloseRunLog();
  await stopProcesses();
  await collectXcresultDiagnostics();
  console.error('🏁 🔴 TEST FAILED — Test #2 stopped before all declared cycles completed; see the gate-specific diagnostics above.');
  process.exit(1);
});
