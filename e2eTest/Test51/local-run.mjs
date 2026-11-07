import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, mkdirSync, readFileSync, statfsSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { batchEnvironmentPaths, createBatchEnvironmentGuard, preflightBatchEnvironment } from '../shared/batch-environment.mjs';
import { discoverAndroidSdk, withAndroidSdkPath } from '../shared/android-sdk.mjs';
import { fingerprintIosCoreSource, prepareIosXcodeBuild, recordIosXcodeBuild, verifyIosXcodeBuild } from '../shared/ios-ffi-provenance.mjs';
import { hashArtifactTree } from '../shared/artifact-tree.mjs';
import { createDiagnosticRedactor } from '../shared/diagnostic-redaction.mjs';
import { prepareScenarioPlatforms, startPlatformLivenessMonitor, stopRunnerOwnedRuntimes } from '../shared/platform-supervisor.mjs';
import { captureExpectedLocalSource, invokeOfficialLocalBuild, recordOfficialNativeBuild, verifyOfficialNativeBuild } from './native-build-checkpoint.mjs';
import { buildRegistrationPlan, validateRegistrationPlan } from './plan.mjs';
import { selectRegistrationExecution } from './execution-selection.mjs';

const directory = dirname(fileURLToPath(import.meta.url));
const e2eRoot = resolve(directory, '..');
const coreRoot = resolve(e2eRoot, '..');
const composeRoot = resolve(coreRoot, '../meta-secret-compose');
export const iosSimulatorUdid = 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30';

export function preparationCommands({ core = coreRoot, compose = composeRoot, e2e = e2eRoot, webOnly = false } = {}) {
  const commands = [
    { stage: 'WASM local', command: '/opt/homebrew/bin/task', args: ['wasm-local'], cwd: core },
    { stage: 'Server local', command: '/opt/homebrew/bin/task', args: ['meta-server'], cwd: core },
    { stage: 'Android app + instrumentation', command: './gradlew', args: [':composeApp:assembleDebug', ':composeApp:assembleDebugAndroidTest', '-PMETA_SECRET_ENV=local', '-PMETA_SECRET_SOCKET_URL=', '-PMETA_SECRET_E2E_SERVER_URL='], cwd: compose },
    { stage: 'iOS app + XCTest', command: 'xcodebuild', args: ['build-for-testing', '-project', resolve(compose, 'iosApp/iosApp.xcodeproj'), '-scheme', 'iosApp', '-configuration', 'Debug', '-destination', `id=${iosSimulatorUdid}`, '-derivedDataPath', resolve(e2e, '.derivedData/iosApp'), 'META_SECRET_ENV=local'], cwd: resolve(compose, 'iosApp'), env: { GRADLE_USER_HOME: resolve(compose, '.gradle-home') } },
  ];
  return webOnly ? commands.slice(0, 2) : commands;
}

/** Existing F2 transaction dispatch; injected effects are only for isolated tooling tests. */
export async function prepareRegistrationEnvironment({ participants, scenario, runId, runDirectory, checkpointPath,
  core = coreRoot, compose = composeRoot, e2e = e2eRoot, env, runCommand, capture,
  checkDisk = () => {}, setStage = () => {}, recordStage = () => {}, onEvent = () => {}, onGuard,
  isInterrupted = () => false, effects = {} }) {
  if (!participants?.size || [...participants].some((platform) => !['web', 'ios', 'android'].includes(platform))) {
    throw new Error('Preparation requires nonempty known participants');
  }
  const webOnly = participants.size === 1 && participants.has('web');
  const operations = { batchEnvironmentPaths, createBatchEnvironmentGuard, preflightBatchEnvironment, discoverAndroidSdk,
    captureExpectedLocalSource, fingerprintIosCoreSource, invokeOfficialLocalBuild, recordOfficialNativeBuild, verifyOfficialNativeBuild,
    prepareIosXcodeBuild, recordIosXcodeBuild, verifyIosXcodeBuild, ...effects };
  const checkInterrupted = () => { if (isInterrupted()) throw new Error('Environment preparation interrupted'); };
  const runtimeScenario = { ...scenario };
  if (!participants.has('android')) delete runtimeScenario.android;
  if (!participants.has('ios')) delete runtimeScenario.ios;
  if (webOnly) {
    // Do not inherit a different run's native readiness or prebuilt claims.
    for (const key of Object.keys(env)) if (key.startsWith('E2E_') && (key.endsWith('_PREBUILT') || key.includes('READINESS'))) delete env[key];
  } else env.PATH = withAndroidSdkPath(env.PATH, operations.discoverAndroidSdk());
  const paths = operations.batchEnvironmentPaths(core, compose);
  paths.artifacts.push(resolve(core, 'meta-secret/Cargo.lock'));
  const expectedWebSource = webOnly ? operations.captureExpectedLocalSource({ coreRoot: core }) : null;
  const controlHashes = webOnly ? ['Taskfile.yml', 'docker-bake.hcl'].map((path) => createHash('sha256').update(readFileSync(resolve(core, path))).digest('hex')) : null;
  const assertWebSource = () => {
    if (operations.fingerprintIosCoreSource(core) !== expectedWebSource.fingerprint
      || ['Taskfile.yml', 'docker-bake.hcl'].some((path, index) => createHash('sha256').update(readFileSync(resolve(core, path))).digest('hex') !== controlHashes[index])) {
      throw new Error('Web preparation source/configuration changed during build');
    }
  };
  let officialBuilt = false;
  const officialBuild = async () => {
    const stage = 'official native build (one time)'; setStage(stage); checkDisk(`before ${stage}`);
    const expectedSource = operations.captureExpectedLocalSource({ coreRoot: core });
    const receipt = await operations.invokeOfficialLocalBuild({ coreRoot: core, runId, runCommand });
    operations.recordOfficialNativeBuild({ coreRoot: core, composeRoot: compose, expectedSource, receipt, checkpointPath });
    checkDisk(`after ${stage}`);
    recordStage({ stage, status: 'passed', finishedAt: new Date().toISOString(), checkpointPath }); officialBuilt = true;
  };
  const guard = operations.createBatchEnvironmentGuard({ ...paths, runDirectory, repositories: [core, compose],
    evidencePath: resolve(runDirectory, 'repository-fingerprints.json'),
    preflight: async () => {
      checkDisk('initial preflight');
      await operations.preflightBatchEnvironment([webOnly ? runtimeScenario : scenario], { runCommand: capture, onEvent });
      if (!webOnly) {
        const devices = JSON.parse((await capture('xcrun', ['simctl', 'list', 'devices', 'available', '-j'])).stdout);
        if (!Object.values(devices.devices).flat().some(({ name, udid }) => name === 'iPhone 17' && udid === iosSimulatorUdid)) throw new Error('Exact approved iPhone17 UDID unavailable; no automatic destination substitution');
      }
    },
    setLocal: webOnly ? async () => {
      checkInterrupted();
      const result = await runCommand('/opt/homebrew/bin/task', ['set-env', '--', 'local', '--selectors-only'], { cwd: core });
      if (result?.exitCode !== 0) throw new Error('Selector-only command did not return a successful exit receipt');
    } : officialBuild,
    isInterrupted, onEvent,
  });
  onGuard(guard);
  await guard.prepare();
  if (webOnly) assertWebSource();
  else {
    // Already-local selectors still need this run's owned native build receipt.
    if (!officialBuilt) await officialBuild();
    operations.verifyOfficialNativeBuild({ coreRoot: core, composeRoot: compose, checkpointPath });
  }
  for (const step of preparationCommands({ core, compose, e2e, webOnly })) {
    checkInterrupted(); setStage(step.stage); checkDisk(`before ${step.stage}`);
    let iosInputs;
    if (step.command === 'xcodebuild') iosInputs = await operations.prepareIosXcodeBuild({ coreRoot: core, composeRoot: compose, derivedDataPath: resolve(e2e, '.derivedData/iosApp'), runAndCapture: capture });
    const result = await runCommand(step.command, step.args, { cwd: step.cwd, env: step.env });
    if (result?.exitCode !== 0) throw new Error(`${step.stage} did not return a successful exit receipt`);
    checkInterrupted();
    if (iosInputs) {
      operations.recordIosXcodeBuild({ coreRoot: core, composeRoot: compose, derivedDataPath: resolve(e2e, '.derivedData/iosApp'), inputs: iosInputs });
      operations.verifyIosXcodeBuild({ derivedDataPath: resolve(e2e, '.derivedData/iosApp') });
    }
    checkDisk(`after ${step.stage}`);
    recordStage({ stage: step.stage, status: 'passed', finishedAt: new Date().toISOString() });
  }
  if (!webOnly) {
    operations.verifyOfficialNativeBuild({ coreRoot: core, composeRoot: compose, checkpointPath });
    return { webOnly, runtimeScenario };
  }
  checkInterrupted(); assertWebSource();
  const wasmRoot = resolve(core, 'meta-secret/web-cli/ui/pkg');
  for (const name of ['meta_secret_wasm.js', 'meta_secret_wasm_bg.wasm']) {
    if (!readFileSync(resolve(wasmRoot, name)).length) throw new Error('Web WASM build output is empty');
  }
  const image = await capture('docker', ['image', 'inspect', scenario.server.image, '--format', '{{.Id}}']);
  if (image.exitCode !== 0 || !image.stdout?.trim()) throw new Error('Web server image identity is missing');
  checkInterrupted(); assertWebSource();
  return { webOnly, runtimeScenario, webProof: { sourceFingerprint: expectedWebSource.fingerprint,
    buildControlSha256: controlHashes, wasmSha256: hashArtifactTree(wasmRoot), serverImageId: image.stdout.trim() } };
}

export async function runLocalRegistration(args = process.argv.slice(2), { beforePreflight = () => {} } = {}) {
  const runId = process.env.E2E_RUN_ID ?? `registration-${Date.now()}`;
  const plan = buildRegistrationPlan(runId);
  const contract = validateRegistrationPlan(plan);
  const selected = selectRegistrationExecution(plan.cases, args);
  const participants = new Set(selected.flatMap((entry) => entry.kind === 'create' ? [entry.platform]
    : [entry.platform, entry.approver, ...(entry.outcome !== 'accepted' ? [entry.retryApprover] : [])]));
  if (args.includes('--validate-only')) {
    console.log('📋 Test51 F1:', contract);
    return 0;
  }
  const runDirectory = resolve(e2eRoot, '.artifacts/runs', runId, 'test51-preparation');
  mkdirSync(runDirectory, { recursive: true });
  const logPath = resolve(runDirectory, 'preparation.log');
  const receiptPath = resolve(runDirectory, 'build-receipt.json');
  const checkpointPath = resolve(runDirectory, 'native-build-checkpoint.json');
  const redactor = createDiagnosticRedactor(Object.entries(process.env).filter(([key]) => /TOKEN|PASSWORD|PRIVATE_KEY|CLIENT_SECRET/i.test(key)).map(([, value]) => value));
  const logLimit = 16 * 1024 * 1024;
  let logged = 0, interrupted, runtimeFailure, currentStage = 'preflight';
  const active = new Set(), processes = [];
  const ownedRuntimes = [];
  let stopMonitor, readiness;
  const writeLog = (text) => { if (logged >= logLimit) return; const safe = text.slice(0, logLimit - logged); appendFileSync(logPath, safe); logged += safe.length; };
  const announce = (message) => { const safe = redactor.redactText(message); console.log(safe); writeLog(`${safe}\n`); };
  const signalGroup = (record, signal) => {
    if (!Number.isInteger(record.child.pid)) return;
    try { process.kill(process.platform === 'win32' ? record.child.pid : -record.child.pid, signal); } catch (error) { if (error.code !== 'ESRCH') throw error; }
  };
  const settleOwnedGroup = async (record) => {
    if (!Number.isInteger(record.child.pid) || process.platform === 'win32') return;
    const alive = () => { try { process.kill(-record.child.pid, 0); return true; } catch (error) { if (error.code === 'ESRCH') return false; throw error; } };
    if (!alive()) return;
    signalGroup(record, 'SIGTERM');
    const started = Date.now();
    while (alive()) {
      if (Date.now() - started > 5000) signalGroup(record, 'SIGKILL');
      if (Date.now() - started > 8000) throw new Error('Owned child process group did not settle before environment restore');
      await new Promise((done) => setTimeout(done, 100));
    }
  };
  const onSignal = (signal) => { interrupted ??= signal; for (const record of active) signalGroup(record, 'SIGTERM'); };
  const signalHandlers = new Map(['SIGINT', 'SIGTERM'].map((signal) => [signal, () => onSignal(signal)]));
  for (const [signal, handler] of signalHandlers) process.on(signal, handler);
  const heartbeat = setInterval(() => announce(`⏳ Test51 ${currentStage}: процесс работает; лог ${logPath}`), 30_000);
  const env = { ...process.env };
  delete env.PUSH_CACHE;
  env.CARGO_BUILD_JOBS ??= '2';
  const runCommand = async (command, commandArgs, options = {}) => {
    if (runtimeFailure) throw runtimeFailure;
    if (interrupted) throw new Error(`Preparation interrupted: ${interrupted}`);
    const { cwd = coreRoot, allowFailure = false, capture = false, timeoutMs = 2_400_000 } = options;
    const child = spawn(command, commandArgs, { cwd, env: { ...env, ...options.env }, detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
    const record = { child, command }; processes.push(record); active.add(record);
    let stdout = '', stderr = '', timedOut = false;
    const streams = { stdout: redactor.createStream(), stderr: redactor.createStream() };
    for (const [name, stream] of [['stdout', child.stdout], ['stderr', child.stderr]]) stream.on('data', (chunk) => {
      if (capture) {
        if (name === 'stdout') stdout = `${stdout}${chunk}`.slice(-8 * 1024 * 1024);
        else stderr = `${stderr}${chunk}`.slice(-65536);
      }
      const safe = streams[name].push(chunk.toString()); writeLog(safe);
      if (!capture && /E2E:|🧭|✅|❌|🟡|🚀|📦|error:|error\[|BUILD (?:FAILED|SUCCESSFUL)|Test51 .*?(?:START|passed|failed|BLOCKED)/.test(safe)) process.stdout.write(safe.slice(-4000));
    });
    const timer = setTimeout(() => { timedOut = true; signalGroup(record, 'SIGTERM'); }, timeoutMs);
    const abortTimer = setTimeout(() => { if (active.has(record)) signalGroup(record, 'SIGKILL'); }, timeoutMs + 10_000);
    record.result = new Promise((done, fail) => { child.once('error', fail); child.once('close', (exitCode, signal) => done({ exitCode, signal })); });
    let result;
    try { result = await record.result; await settleOwnedGroup(record); } finally {
      clearTimeout(timer); clearTimeout(abortTimer); active.delete(record);
      for (const stream of Object.values(streams)) writeLog(stream.flush());
    }
    if (interrupted || timedOut || (!allowFailure && result.exitCode !== 0)) throw new Error(`${command} failed (${interrupted ?? (timedOut ? 'deadline' : result.exitCode ?? result.signal)}); see redacted ${logPath}`);
    return { ...result, stdout, stderr };
  };
  const capture = (command, commandArgs, options = {}) => runCommand(command, commandArgs, { ...options, capture: true, timeoutMs: options.timeoutMs ?? 30_000 });
  const scenario = JSON.parse(readFileSync(resolve(directory, 'scenario.json'), 'utf8'));
  const receipt = { runId, startedAt: new Date().toISOString(), stages: [], scope: 'registration-only', requestedCases: selected.length };
  const save = () => writeFileSync(receiptPath, `${JSON.stringify(receipt, null, 2)}\n`);
  const checkDisk = (boundary) => {
    const disk = statfsSync(coreRoot); const freeGiB = disk.bavail * disk.bsize / 1024 ** 3;
    receipt.diskChecks ??= [];
    receipt.diskChecks.push({ boundary, freeGiB, at: new Date().toISOString() }); save();
    if (freeGiB < 12) throw new Error(`Disk ${freeGiB.toFixed(2)} GiB at ${boundary}: below 12GiB floor; stop and restore, no active-cache cleanup`);
    if (freeGiB < 15) throw new Error(`Disk ${freeGiB.toFixed(2)} GiB at ${boundary}: approved allowlisted cleaner required before continuing; restore first, no active-cache cleanup`);
    announce(`🧹 Test51 ${boundary}: ${freeGiB.toFixed(2)} GiB free; cleanup skipped`);
  };
  let guard, resultCode = 1, failure;
  try {
    await beforePreflight({ selected, participants });
    const preparation = await prepareRegistrationEnvironment({ participants, scenario, env, runId, runDirectory, checkpointPath,
      runCommand, capture, checkDisk, onEvent: announce, isInterrupted: () => Boolean(interrupted),
      onGuard: (value) => { guard = value; },
      setStage: (stage) => { currentStage = stage; announce(`⚙️ Test51 ${stage}`); },
      recordStage: (stage) => { receipt.stages.push(stage); save(); },
    });
    if (preparation.webOnly) { receipt.preparationScope = 'web'; receipt.webProof = preparation.webProof; save(); }
    checkDisk('before real Core UI cases');
    if (!preparation.webOnly) {
      currentStage = 'F3 native runtime readiness';
      readiness = await prepareScenarioPlatforms({ scenario: preparation.runtimeScenario, testNumber: 51, attempt: 1, runId, runDirectory,
        onRuntimeStarted: (runtime) => ownedRuntimes.push(runtime) });
      receipt.readinessPath = readiness.reportPath; receipt.ownedRuntimes = ownedRuntimes;
      stopMonitor = startPlatformLivenessMonitor({ platforms: readiness.platforms, testNumber: 51, unavailableGraceMs: 0, onEvent: announce,
        onFailure: (error) => { runtimeFailure = error; for (const record of active) signalGroup(record, 'SIGTERM'); } });
    }
    receipt.status = 'prepared'; save();
    currentStage = 'real Core UI cases'; announce('🚀 Test51 preparation passed; real Core UI cases start');
    const business = await runCommand(process.execPath, [resolve(directory, 'orchestrator.mjs'), ...args], { cwd: e2eRoot, allowFailure: true, timeoutMs: 7_200_000,
      env: { E2E_RUN_ID: runId, E2E_RANGE_RUN: '1', E2E_REUSE_PROVEN_ARTIFACTS: '1', E2E_BOOTSTRAP_BUILD_PROVENANCE: '1', E2E_BUILD_PROVENANCE_EVIDENCE: receiptPath,
        ...(!preparation.webOnly ? { E2E_ANDROID_FFI_PREBUILT: '1', E2E_READINESS_PATH: readiness.reportPath } : {}) } });
    if (runtimeFailure) throw runtimeFailure;
    resultCode = business.exitCode ?? 1;
  } catch (error) { failure = error; receipt.status = 'failed'; receipt.error = redactor.redactText(error.message); save(); }
  finally {
    currentStage = 'cleanup and exact restore';
    try {
      for (const record of active) signalGroup(record, 'SIGTERM');
      const force = setTimeout(() => { for (const record of active) signalGroup(record, 'SIGKILL'); }, 10_000);
      await Promise.allSettled(processes.map(({ result }) => result).filter(Boolean)); clearTimeout(force);
      const settled = await Promise.allSettled(processes.map(settleOwnedGroup));
      const unsettled = settled.filter(({ status }) => status === 'rejected');
      stopMonitor?.();
      try { await stopRunnerOwnedRuntimes(ownedRuntimes, announce); } catch (error) { unsettled.push({ reason: error }); }
      try { await guard?.restore(); receipt.restored = true; } catch (error) { receipt.restored = false; unsettled.push({ reason: error }); }
      if (unsettled.length) throw new AggregateError(unsettled.map(({ reason }) => reason), 'Owned process cleanup failed');
    } catch (error) { failure = new AggregateError([...(failure ? [failure] : []), error], 'Test51 cleanup/restore failed'); resultCode = 1; receipt.status = 'failed'; receipt.restoreError = redactor.redactText(error.message); }
    clearInterval(heartbeat); for (const [signal, handler] of signalHandlers) process.off(signal, handler);
    receipt.finishedAt = new Date().toISOString(); receipt.exitCode = failure ? 1 : resultCode; save();
  }
  if (failure) throw failure;
  announce(`📦 Test51 exit=${resultCode}; original environment restored; evidence ${receiptPath}`);
  return resultCode;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  runLocalRegistration().then((code) => { process.exitCode = code; }).catch((error) => { console.error(`❌ ${error.message}`); process.exitCode = 1; });
}
