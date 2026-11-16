import { createIosSelectedSuiteActor } from '../shared/ios-selected-suite.mjs';
import { createHarnessShutdown, formatHarnessShutdownError } from '../shared/harness-shutdown.mjs';
import { createDiagnosticRedactor } from '../shared/diagnostic-redaction.mjs';
import { resetE2eNativeAppState } from '../shared/e2e-state-reset.mjs';
import { prepareIosCallerIdentity, prepareScopedIosInvocation, iosGeneratedTemplateProof } from '../shared/ios-test-identity.mjs';
import { assertExistingDeviceRun, readExistingAndroidAvd, iosSerialTestArguments, resolveIosScenarioSimulator, resolveAndroidScenarioAvdName, verifyOwnedAndroidScenarioRuntime } from '../shared/platform-supervisor.mjs';
import { loadBatchProducts, assertBatchProductsCurrent } from '../shared/batch-product-consumer.mjs';
import { androidInstrumentationAdbArguments, instrumentationArgumentsFromGradle, superviseAndroidInstrumentation, assertAndroidInstrumentationPassed } from '../shared/android-instrumentation.mjs';
import { openWebSecrets, hideWebSecret } from '../shared/web-liveboard-actions.mjs';
import { approveWebJoin, openWebIncoming, decideWebIncoming, waitWebIncomingGone } from '../shared/web-liveboard-inbox.mjs';
import { requestWebVaultJoin, waitForWebEntry } from '../shared/web-orbit-registration.mjs';
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
const artifactsDirectory = resolve(e2eRoot, '.artifacts');
const testAttemptToken = process.env.E2E_RUN_ID
  ? `${process.env.E2E_RUN_ID}-attempt-${process.env.E2E_TEST_ATTEMPT ?? '1'}`
  : new Date().toISOString().replace(/[:.]/g, '-');
const iosResultBundlePath = resolve(artifactsDirectory, `test-3-run-${testAttemptToken}.xcresult`);
const androidSdkTools = discoverAndroidSdk();
const androidEmulatorPath = androidSdkTools.emulatorPath;
const androidTestBundleId = `${scenario.android.bundleId}.test`;
const serverContainer = scenario.server.container;
const serverImage = scenario.server.image;
let batchProducts;
let iosIdentity;
const namespaceRedactor = createDiagnosticRedactor();
// Outer failure rendering is separate from the namespace-only marker stream.
const errorRedactor = createDiagnosticRedactor([scenario.secret.value]);
const processes = [];
const ownedChildClosures = new Map();
const iosSuiteActors = [];
let browserInstance;
let browserCloseRequested = false;
let browserLifecyclePhase = 'NOT_CREATED';
let nativeBusinessPromise;
let nativeBusinessFailure;
const watchedProcessOutputs = [];
const browserDiagnostics = [];
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
  harnessShutdown.assertCanSpawn();
  const child = spawn(command, args, {
    cwd: options.cwd ?? projectRoot,
    env: {
      ...process.env,
      PATH: withAndroidSdkPath([process.env.PATH, '/opt/homebrew/bin', '/usr/local/bin'].filter(Boolean).join(':'), androidSdkTools),
      ...(options.env ?? {}),
    },
    stdio: options.stdio ?? 'inherit',
    shell: false,
  });
  ownedChildClosures.set(child, new Promise(resolveClose => child.once('close', resolveClose)));
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

  const { iosIdentityRedactor, ...childOptions } = options;
  const streams = iosIdentityRedactor ? { stdout: iosIdentityRedactor.createStream(), stderr: iosIdentityRedactor.createStream() } : null;
  const child = run(command, args, { ...childOptions, stdio: ['ignore', 'pipe', 'pipe'] });
  const processOutput = (chunk, sink, flushed = false) => {
    const text = streams && !flushed ? streams[sink].push(chunk.toString()) : chunk.toString();
    if (!text) return;
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
  child.once('close', () => { if (streams) for (const sink of ['stdout', 'stderr']) processOutput(streams[sink].flush(), sink, true); });

  let processError;
  const cancelPendingWaits = error => {
    processError ??= error;
    for (const waiter of markerWaiters) {
      clearTimeout(waiter.timeout);
      waiter.reject(processError);
    }
    markerWaiters.clear();
  };
  const result = new Promise((resolvePromise, reject) => {
    child.once('error', error => { cancelPendingWaits(error); reject(error); });
    child.once('exit', (code, signal) => {
      processError = code === 0 ? new Error(`${command} ended before expected E2E event`) : new Error(`${command} exited with ${code ?? signal}`);
      cancelPendingWaits(processError);
      if (code === 0) resolvePromise({ stdout, stderr });
      else reject(processError);
    });
  });
  const closed = new Promise((resolvePromise) => {
    child.once('close', (code, signal) => resolvePromise({ code, signal }));
  });

  const waitForMarker = (marker, timeoutMs = 300_000) => new Promise((resolvePromise, reject) => {
    if (`${stdout}\n${stderr}`.includes(marker)) return resolvePromise();
    if (processError) return reject(processError);
    const waiter = {
      marker,
      resolve: resolvePromise,
      reject: reject,
      timeout: timeoutMs === null ? null : setTimeout(() => {
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
    cancelPendingWaits,
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
  if (browserDiagnostics.length > 0) {
    console.error(`\n=== E2E diagnostic: Web browser ===\n${browserDiagnostics.slice(-200).join('\n')}`);
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

async function performCleanup() {
  await approvalCoordinator?.close().catch(() => {});
  approvalCoordinator = undefined;
  for (const child of processes.reverse()) {
    if (!child.killed) child.kill('SIGTERM');
  }
  await wait(500);
  await runAndWait('docker', ['rm', '-f', serverContainer], { stdio: 'ignore' }).catch(() => {});
}

const harnessShutdown = createHarnessShutdown({
  async cancelActors(error = new Error('Scenario actors finished')) {
    nativeBusinessFailure ??= error;
    const failures = [];
    for (const watched of watchedProcessOutputs) watched.cancelPendingWaits(error);
    const iosChildren = new Set(iosSuiteActors.map(actor => actor.child));
    const joins = iosSuiteActors.map(actor => actor.cancelAndJoin(error));
    const ownedBrowser = browserInstance; browserInstance = undefined;
    const closingBrowser = Promise.resolve().then(() => {
      browserCloseRequested = true;
      browserLifecyclePhase = 'OWNER_CLOSE';
      return ownedBrowser?.close();
    });
    void closingBrowser.catch(() => {});
    for (const child of processes) {
      try { if (!iosChildren.has(child) && child.exitCode == null && child.signalCode == null) child.kill('SIGTERM'); }
      catch (failure) { failures.push(failure); }
    }
    const within = async (promise, duration) => {
      let timer;
      try { return await Promise.race([promise.then(() => true), new Promise(done => { timer = setTimeout(() => done(false), duration); })]); }
      finally { clearTimeout(timer); }
    };
    // iOS has its own original5/5/2 join, including any terminal result flush.
    const closed = Promise.all([...ownedChildClosures].filter(([child]) => !iosChildren.has(child)).map(([, close]) => close));
    if (!await within(closed, 5_000)) {
      for (const child of processes) {
        try { if (!iosChildren.has(child) && child.exitCode == null && child.signalCode == null) child.kill('SIGKILL'); }
        catch (failure) { failures.push(failure); }
      }
      if (!await within(closed, 2_000)) failures.push(new Error('Exact owned scenario children did not close'));
    }
    for (const result of await Promise.allSettled(joins)) if (result.status === 'rejected') failures.push(result.reason);
    if (!await within(closingBrowser.catch(error => { failures.push(error); }), 5_000)) {
      failures.push(Object.assign(new Error('Owned browser close remains unproven after 5000ms'),
        { code: 'OWNED_TEARDOWN_INCOMPLETE', stage: 'browser-close' }));
    }
    if (!await within(Promise.resolve(nativeBusinessPromise).catch(() => {}), 5_000)) {
      failures.push(Object.assign(new Error('Owned native business continuation remains unjoined after 5000ms'),
        { code: 'OWNED_TEARDOWN_INCOMPLETE', stage: 'business-join' }));
    }
    if (failures.length) throw new AggregateError(failures, 'Native business cancellation failed', { cause: failures[0] });
  },
  captureDiagnostics: printFailureDiagnostics,
  cleanup: performCleanup,
});
function stopProcesses() { return harnessShutdown.finish(); }

async function findIosSimulator() {
  const { stdout } = await runAndCapture('xcrun', ['simctl', 'list', 'devices', 'available', '-j']);
  return resolveIosScenarioSimulator({ simulatorName: scenario.ios.simulatorName, inventory: JSON.parse(stdout) });
}

async function prepareIosSimulator() {
  const simulator = await findIosSimulator();
  console.log(`8. Preparing iOS simulator: ${simulator.name} (${simulator.udid})`);
  if (simulator.state !== 'Booted') {
    await runAndWait('xcrun', ['simctl', 'boot', simulator.udid]);
  }
  await runAndWait('xcrun', ['simctl', 'bootstatus', simulator.udid, '-b']);
  if (process.env.E2E_RANGE_RUN !== '1') {
    await resetE2eNativeAppState({ scenario, platforms: [{ platform: 'iOS', simulatorName: simulator.name, udid: simulator.udid }],
      iosIdentityContext: iosIdentity.context, runCommand: runAndCapture });
  }
  await openVisibleIosDeviceUi({ runAndCapture, runAndWait, simulatorUdid: simulator.udid });
  return simulator.udid;
}

function startIosJoinTest(simulatorUdid) {
  assertBatchProductsCurrent({ products: batchProducts, coreRoot, composeRoot });
  verifyIosXcodeBuild({ derivedDataPath: iosDerivedDataPath });
  const actorEnvironment = {
        E2E_VAULT_NAME: scenario.vault.name,
        E2E_SECRET_NAME: scenario.secret.name,
        E2E_SECRET_VALUE: scenario.secret.value,
        E2E_RECOVERY_CYCLES: String(recoveryCycles.length),
        E2E_IOS_RECOVERY_APPROVALS: iosRecoveryApprovalCycles.join(','),
        E2E_APPROVAL_COORDINATOR_URL: 'http://127.0.0.1:5180',
      };
  const scoped = prepareScopedIosInvocation({ identityContext: iosIdentity.context,
    templateProof: iosGeneratedTemplateProof(iosDerivedDataPath), selector: 'iosAppUITests/CaseThreeIosJoinUITest/testJoinAndroidInitiatedVaultAndHandleRecovery',
    hostEnvironment: Object.fromEntries(Object.entries(actorEnvironment).filter(([, value]) => value !== undefined).map(([key, value]) => [key, String(value)])),
    authPolicy: 'controlled',
    outputDirectory: resolve(iosIdentity.context.runRoot, 'ios-invocations') });
  console.log('9. Starting iOS UI test');
  const watched = watchProcessOutput(
    'xcodebuild',
    [
      'test-without-building', '-xctestrun', scoped.xctestrun,
      ...iosSerialTestArguments(simulatorUdid),
      '-resultBundlePath',
      iosResultBundlePath,
      '-only-testing:iosAppUITests/CaseThreeIosJoinUITest/testJoinAndroidInitiatedVaultAndHandleRecovery',
    ],
    {
      cwd: resolve(composeRoot, 'iosApp'),
      iosIdentityRedactor: namespaceRedactor,
      env: actorEnvironment,
    },
  );
  const selected = createIosSelectedSuiteActor({ watched,
    selector: 'iosAppUITests/CaseThreeIosJoinUITest/testJoinAndroidInitiatedVaultAndHandleRecovery', timeoutMs: 600_000, deferCompletionTimeout: true });
  const actor = { ...watched, ...selected };
  iosSuiteActors.push(actor);
  return actor;
}

async function prepareAndroidEmulator() {
  let serial = await findOnlineAndroidAvd(androidSdkTools, resolveAndroidScenarioAvdName(scenario.android.avdName), runAndCapture);
  if (process.env.E2E_ANDROID_OWNED_PROFILE !== undefined) {
    await verifyOwnedAndroidScenarioRuntime(scenario.android.avdName, serial, { runCommand: runAndCapture });
  }

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
    await runAndWait('adb', ['-s', serial, 'uninstall', androidTestBundleId], { stdio: 'ignore' }).catch(() => {});
  }
  return serial;
}

async function waitForConfiguredAndroidAvd(timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = `AVD ${scenario.android.avdName} is not online`;
  while (Date.now() < deadline) {
    try {
      const serial = await findOnlineAndroidAvd(androidSdkTools, resolveAndroidScenarioAvdName(scenario.android.avdName), runAndCapture);
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
  if (!avds.includes(resolveAndroidScenarioAvdName(scenario.android.avdName))) {
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
      if (nativeBusinessFailure) throw nativeBusinessFailure;
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
  if (batchProducts) {
    batchProducts = await loadBatchProducts({ coreRoot, composeRoot, scenario, testNumber: 3, runAndCapture });
    await installAndVerifyAndroidE2eApps({ serial, appBundleId: scenario.android.bundleId, composeRoot, prebuiltApks: batchProducts.android.products, verifiedBuild: batchProducts.android, runAndWait, runAndCapture });
  }
  // An emulator can briefly switch from `device` to `offline` after boot
  // while its services settle. AGP fails immediately in that state, so make
  // the state check immediately before handing control to Gradle.
  await waitForAndroidBoot(serial);
  // connectedDebugAndroidTest installs both the app and its instrumentation
  // APK, then launches the target activity itself. A separate `am start`
  // preflight is redundant and has proved flaky on the Android 16 emulator.
  await runAdbWithRetry(['-s', serial, 'logcat', '-c']);
  const logcat = watchProcessOutput('adb', ['-s', serial, 'logcat', 'MetaSecretE2E:I', '*:S']);
  void logcat.result.catch(() => {}); // logcat is stopped deliberately when the instrumentation test ends
  const runnerGradleArguments = [
      ':composeApp:installDebug',
      ':composeApp:connectedDebugAndroidTest',
      '-Pandroid.testInstrumentationRunnerArguments.class=metasecret.project.com.CaseThreeAndroidInitiatorTest#createVaultThenApproveWebAndIos',
      `-Pandroid.testInstrumentationRunnerArguments.vaultName=${scenario.vault.name}`,
      `-Pandroid.testInstrumentationRunnerArguments.secretName=${scenario.secret.name}`,
      `-Pandroid.testInstrumentationRunnerArguments.recoveryCycles=${recoveryCycles.length}`,
      `-Pandroid.testInstrumentationRunnerArguments.androidRecoveryApprovals=${androidRecoveryApprovalCycles.join(',')}`,
      '-Pandroid.testInstrumentationRunnerArguments.approvalCoordinatorUrl=http://10.0.2.2:5180',
    ];
  if (batchProducts) {
    const logical = instrumentationArgumentsFromGradle(runnerGradleArguments);
    const actor = watchProcessOutput('adb', androidInstrumentationAdbArguments(serial, logical, `${scenario.android.bundleId}.test/androidx.test.runner.AndroidJUnitRunner`));
    return superviseAndroidInstrumentation({ instrumentationResult: actor.result, logcat, selector: logical[logical.indexOf('class') + 1], validateOutput: assertAndroidInstrumentationPassed });
  }
  const test = runAndWait(
    './gradlew',
    runnerGradleArguments,
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
    waitForWebEntry(page),
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

  await waitForWebEntry(page);
}

async function approveJoinRequestOnWeb(page, deviceName) {
  await page.bringToFront();
  console.log(`Web approving ${deviceName} join request`);
  await approveWebJoin(page, deviceName);
}

async function closeWebSecret(page) {
  await hideWebSecret(page, scenario.secret.name, scenario.secret.value, { timeout: recoveryShowTimeoutMs });
}

async function runRecoveryCyclesFromAndroid(page, iosTest, androidTest) {
  iosTest.assertBusinessActive();
  await openWebSecrets(page);

  let previousApprover;
  for (const cycle of recoveryCycles) {
    iosTest.assertBusinessActive();
    if (cycle.approver !== previousApprover) {
      console.log(`📱 Roles changed: Device A — Web ${cycle.approver === 'web' ? 'approver' : 'observer'}; Device B — iOS ${cycle.approver === 'ios' ? 'approver' : 'observer'}; Device C — Android requester`);
      previousApprover = cycle.approver;
    }
    console.log(`15.${cycle.number} Android requesting secret recovery (${cycle.approver} approves)`);
    iosTest.assertBusinessActive();
    approvalCoordinator.allow('android-sender', cycle.number);
    await androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_REQUEST_SENT_${cycle.number}`);
    // An incoming recovery request changes this <li>'s ARIA role from listitem
    iosTest.assertBusinessActive();
    const openRequest = await openWebIncoming(page, { kind: 'recover', subject: scenario.secret.name, count: 1 });
    console.log(`Web received recovery ${cycle.number}; opening its recovery request`);
    // Real04 is expanded by openWebIncoming; observers make zero decisions.

    await iosTest.waitForMarker(`E2E: IOS_RECOVERY_REQUEST_ALERT_${cycle.number}`);
    if (cycle.approver === 'web') {
      console.log(`Web approving recovery ${cycle.number}`);
      iosTest.assertBusinessActive();
      await decideWebIncoming(page, { kind: 'recover', requestId: openRequest.requestId, subject: scenario.secret.name, count: 1 }, true, { waitForGone: false });
    } else {
      iosTest.assertBusinessActive();
      approvalCoordinator.allow('ios', cycle.number);
      await iosTest.waitForMarker(`E2E: IOS_RECOVERY_APPROVE_SUCCESS_${cycle.number}`);
    }

    await androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_SECRET_VISIBLE_${cycle.number}`, recoveryShowTimeoutMs);
    console.log(`✅ Recovery ${cycle.number}/${recoveryCycles.length}: Android show secret passed`);
    await androidTest.waitForMarker(`E2E: ANDROID_RECOVERY_CLOSED_${cycle.number}`);
    // A badge is keyed by the secret, not by recovery cycle. Do not let the
    // next request recreate it before iOS has observed the previous badge's
    // disappearance and advanced its XCTest loop.
    await iosTest.waitForMarker(`E2E: IOS_RECOVERY_REQUEST_CLOSED_${cycle.number}`);
    await waitWebIncomingGone(page, 'recover', scenario.secret.name);

  }
}

async function runIosJoinAndRecovery(page, iosTest, androidTest) {
  iosTest.assertBusinessActive();
  await iosTest.waitForMarker('E2E: IOS_JOIN_REQUEST_SENT');
  await androidTest.waitForMarker('E2E: ANDROID_IOS_JOIN_APPROVED', 120_000);
  // Let iOS finish its own join timeout (and emit a useful failure) before
  // stopping the parallel Android instrumentation.
  await iosTest.waitForMarker('E2E: IOS_SECRET_VISIBLE', 210_000);

  iosTest.assertBusinessActive();
  await runRecoveryCyclesFromAndroid(page, iosTest, androidTest);
  iosTest.assertBusinessActive();
  await Promise.all([iosTest.complete(), androidTest.result]);

}

async function main() {
  assertExistingDeviceRun({ scenarios: [scenario] });
  iosIdentity = prepareIosCallerIdentity({ scenario, scenarioPath, testNumber: 3,
    attemptToken: new Date().toISOString().replace(/[:.]/g, '-'), artifactsDirectory });
  namespaceRedactor.addSensitiveValues([iosIdentity.scope.namespace]);
  errorRedactor.addSensitiveValues([iosIdentity.scope.namespace]);
  if (scenario.android) readExistingAndroidAvd(scenario.android.avdName);
  batchProducts = await loadBatchProducts({ coreRoot, composeRoot, scenario, testNumber: 3, runAndCapture });
  if (!existsSync(webDirectory)) throw new Error(`Web directory not found: ${webDirectory}`);
  if (!existsSync(composeRoot)) throw new Error(`Compose directory not found: ${composeRoot}`);

  console.log(`\n=== ${scenario.name} ===`);
  await ensureIosFfi({ coreRoot, composeRoot, runAndWait, evidenceDirectory: artifactsDirectory });
  await preflightLocalRuntimes();
  approvalCoordinator = await startApprovalCoordinator();
  console.log('1. Rebuilding the Web WASM package');
  if (!batchProducts) await runAndWait('/opt/homebrew/bin/task', ['wasm-local'], { cwd: coreRoot, env: { PUSH_CACHE: '' } });

  console.log('2. Building local server image');
  if (!batchProducts) await runAndWait('/opt/homebrew/bin/task', ['meta-server'], { cwd: coreRoot });

  console.log('3. Cleaning server state');
  await runAndWait('docker', ['rm', '-f', serverContainer], { stdio: 'ignore' }).catch(() => {});
  console.log('4. Starting local server');
  const server = watchProcessOutput('docker', ['run', '--rm', '--name', serverContainer, '-p', `${scenario.server.port}:3000`, serverImage]);
  void server.result.catch(() => {});
  await waitForHttp(scenario.server.url);

  console.log('5. Starting Web in a visible browser');
  run('npm', ['run', 'dev', '--', '--host', 'localhost', '--port', '5173'], { cwd: webDirectory });
  await waitForHttp(scenario.web.url);

  const browser = browserInstance = await chromium.launch({ headless: false });
  browserLifecyclePhase = 'BROWSER_LAUNCHED';
  let page;
  let lifecycleSequence = 0;
  const lifecyclePhases = new Set(['BROWSER_LAUNCHED', 'PAGE_READY', 'NATIVE_PREP',
    'AWAIT_ANDROID_INITIATOR', 'BEFORE_GOTO', 'AFTER_GOTO', 'OWNER_CLOSE']);
  const observeLifecycle = event => {
    try {
      if (lifecycleSequence >= 8) return;
      const time = Date.now(), connected = browser.isConnected(), pageClosed = page?.isClosed();
      const record = { SEQ: ++lifecycleSequence, TIME_MS: Number.isSafeInteger(time) ? time : null,
        PHASE: lifecyclePhases.has(browserLifecyclePhase) ? browserLifecyclePhase : 'UNKNOWN', EVENT: event,
        REQUESTED_CLOSE: browserCloseRequested ? 1 : 0,
        CONNECTED: typeof connected === 'boolean' ? Number(connected) : null,
        PAGE_CLOSED: typeof pageClosed === 'boolean' ? Number(pageClosed) : null };
      const line = errorRedactor.redactText(`BROWSER_LIFECYCLE ${JSON.stringify(record)}`);
      // Fixed fields and redaction replacements are ASCII; retain at most8 records of512 bytes.
      if (line.length <= 512) browserDiagnostics.push(line);
    } catch { /* Observation must never replace a scenario failure. */ }
  };
  try { browser.on('disconnected', () => observeLifecycle('BROWSER_DISCONNECTED')); } catch { /* Diagnostic only. */ }
  page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  try { page.context().on('close', () => observeLifecycle('CONTEXT_CLOSED')); } catch { /* Diagnostic only. */ }
  try {
    page.on('close', () => observeLifecycle('PAGE_CLOSED'));
    page.on('crash', () => observeLifecycle('PAGE_CRASHED'));
  } catch { /* Diagnostic only. */ }
  browserLifecyclePhase = 'PAGE_READY';
  page.on('console', (message) => {
    browserDiagnostics.push(`[${message.type()}] ${message.text()}`);
  });
  page.on('pageerror', (error) => {
    browserDiagnostics.push(`[pageerror] ${error.message}`);
  });
  await setupVirtualAuthenticator(page);

  browserLifecyclePhase = 'BEFORE_GOTO';
  await page.goto(scenario.web.url, { waitUntil: 'networkidle' });
  browserLifecyclePhase = 'AFTER_GOTO';
  browserLifecyclePhase = 'NATIVE_PREP';
  const androidSerial = await prepareAndroidEmulator();
  console.log('5a. Installing and verifying Android app and instrumentation before scenario actions');
  await installAndVerifyAndroidE2eApps({
    serial: androidSerial,
    appBundleId: scenario.android.bundleId,
    composeRoot,
    ...(batchProducts ? { prebuiltApks: batchProducts.android.products, verifiedBuild: batchProducts.android } : {}),
    runAndWait,
    runAndCapture,
  });
  console.log('5b. Preparing iOS app and XCTest runner before scenario actions');
  mkdirSync(artifactsDirectory, { recursive: true });
  const simulatorUdid = await prepareIosSimulator();
  const iosReadiness = await prepareIosTestApps({
    simulatorUdid,
    projectPath: iosProjectPath,
    verifiedBuild: batchProducts?.ios,
    derivedDataPath: iosDerivedDataPath,
    appBundleId: scenario.ios.bundleId,
    testSelector: 'iosAppUITests/CaseThreeIosJoinUITest/testJoinAndroidInitiatedVaultAndHandleRecovery',
    cwd: resolve(composeRoot, 'iosApp'),
    runAndWait,
    runAndCapture,
  });
  console.log(`✅ iOS app and XCTest runner are installed (${iosReadiness.appBundleId})`);

  const androidTest = await startAndroidJoinTest(androidSerial);
  browserLifecyclePhase = 'AWAIT_ANDROID_INITIATOR';
  await androidTest.waitForMarker('E2E: ANDROID_INITIATOR_READY');

  console.log('Web joining Android-created vault');
  await unlockWithPasskeyIfNeeded(page);
  await requestWebVaultJoin(page, scenario.vault.name);
  await androidTest.waitForMarker('E2E: ANDROID_WEB_JOIN_APPROVED', 120_000);
  await openWebSecrets(page);

  const iosTest = startIosJoinTest(simulatorUdid);
  nativeBusinessPromise = runIosJoinAndRecovery(page, iosTest, androidTest);
  try { await Promise.race([nativeBusinessPromise, iosTest.failure]); }
  catch (primary) {
    try { await harnessShutdown.finish({ error: primary }); }
    catch (cleanup) { throw new AggregateError([primary, cleanup], 'Native business failed and cancellation failed', { cause: primary }); }
    throw primary;
  }

  iosTest.assertBusinessActive();
  console.log('✅ Test #3 Web + iOS + Android part passed');
  if (exitOnSuccess) {
    browserCloseRequested = true;
    browserLifecyclePhase = 'OWNER_CLOSE';
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
  console.error(`\n❌ Test #3 failed: ${formatHarnessShutdownError(error, errorRedactor.redactText)}`);
  printFailureDiagnostics();
  try { await stopProcesses(); }
  catch (cleanup) { console.error(`Owned cleanup failed after primary: ${formatHarnessShutdownError(cleanup, errorRedactor.redactText)}`); }
  process.exit(1);
});
