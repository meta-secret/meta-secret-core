import { scenarioProgressLine } from './shared/scenario-progress.mjs';
import { batchEnvironmentPaths, createBatchEnvironmentGuard, preflightBatchEnvironment } from './shared/batch-environment.mjs';
import { spawn, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { arch, platform, release, tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { ensureIosFfi } from './shared/ios-ffi-provenance.mjs';
import { createDiagnosticRedactor } from './shared/diagnostic-redaction.mjs';
import {
  prepareScenarioPlatforms,
  startPlatformLivenessMonitor,
  stopRunnerOwnedRuntimes,
} from './shared/platform-supervisor.mjs';
import { resetE2eScenarioState } from './shared/e2e-state-reset.mjs';
import { RunArtifactCollector } from './shared/run-artifacts.mjs';

const e2eRoot = dirname(fileURLToPath(import.meta.url));
const packageJsonPath = resolve(e2eRoot, 'package.json');
const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
const testsRoot = resolve(e2eRoot, '.artifacts', 'runs');
const coreRoot = resolve(e2eRoot, '..');
const composeRoot = resolve(coreRoot, '..', 'meta-secret-compose');
const metaCliRoot = resolve(coreRoot, 'meta-secret');
const metaCliBinaryPath = resolve(metaCliRoot, 'target/debug/meta-cli');
const androidFfiBuildScript = resolve(coreRoot, 'meta-secret/mobile/scripts/build-android.sh');
const androidFfiProvenancePath = resolve(e2eRoot, '.artifacts/android-ffi-build-provenance.json');
const androidFfiMarker = '[CORE_DIAG] sync_vault response events=';
const diskThresholdGiB = 15;
const postCleanupMinimumGiB = 12;
let activeChild;
let preparationKillTimer;
let interruptedBy;
let androidFfiSourcePrepared;

function dirname(path) {
  return resolve(path, '..');
}

function fingerprintAndroidCoreSource() {
  const listed = spawnSync('git', ['ls-files', '-co', '--exclude-standard', '-z', '--', 'meta-secret'], {
    cwd: coreRoot,
    encoding: 'utf8',
  });
  if (listed.status !== 0) throw new Error(`Could not enumerate Core source for Android FFI provenance: ${listed.stderr}`);
  const digest = createHash('sha256');
  for (const relativePath of listed.stdout.split('\0').filter(Boolean).sort()) {
    const path = resolve(coreRoot, relativePath);
    if (!existsSync(path)) continue;
    digest.update(`${relativePath}\0`);
    digest.update(readFileSync(path));
  }
  return digest.digest('hex');
}

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

async function prepareAndroidFfiBeforePlatforms(entry, collector) {
  if (!entry.scenarios.some(({ value }) => value.android)) return false;
  const sourceFingerprint = fingerprintAndroidCoreSource();
  if (androidFfiSourcePrepared === sourceFingerprint) {
    collector.writeEvent(`Android FFI preflight reused verified build source=${sourceFingerprint}`);
    return true;
  }
  if (androidFfiSourcePrepared && androidFfiSourcePrepared !== sourceFingerprint) {
    throw new Error('Core source changed during this E2E batch after simulators started; finish or restart the batch before rebuilding Android FFI');
  }
  if (!existsSync(androidFfiBuildScript)) throw new Error(`Official Android FFI build script not found: ${androidFfiBuildScript}`);
  collector.writeEvent(`building Android Core FFI before platform startup for source=${sourceFingerprint}`);
  process.stdout.write(`🧭 TEST #${entry.testNumber}: building Android Core FFI before launching simulators\n`);
  await runPreparationCommand('bash', [androidFfiBuildScript], { cwd: coreRoot, timeoutMs: 2400000, onOutput: (stream, chunk) => collector.writeOutput(stream, chunk) });

  const abiHashes = {};
  for (const [abi, folder] of [['arm64-v8a', 'arm64-v8a'], ['armeabi-v7a', 'armeabi-v7a'], ['x86_64', 'x86_64']]) {
    const libraryPath = resolve(composeRoot, `composeApp/src/androidMain/jniLibs/${folder}/libmetasecret_mobile.so`);
    if (!existsSync(libraryPath)) throw new Error(`Android Core FFI preflight output missing for ABI ${abi}`);
    const library = readFileSync(libraryPath);
    if (!library.includes(Buffer.from(androidFfiMarker))) throw new Error(`Android Core FFI preflight lacks required Core diagnostic marker for ABI ${abi}`);
    abiHashes[abi] = sha256(library);
  }
  writeFileSync(androidFfiProvenancePath, `${JSON.stringify({
    version: 1,
    builtAt: new Date().toISOString(),
    sourceFingerprint,
    diagnosticMarker: androidFfiMarker,
    abiSha256: abiHashes,
  }, null, 2)}\n`);
  androidFfiSourcePrepared = sourceFingerprint;
  collector.writeEvent(`Android Core FFI ready before simulator startup source=${sourceFingerprint} arm64-v8a=${abiHashes['arm64-v8a']} armeabi-v7a=${abiHashes['armeabi-v7a']} x86_64=${abiHashes.x86_64} diagnosticMarker=true`);
  process.stdout.write(`🧭 TEST #${entry.testNumber}: Android Core FFI built and verified before simulator startup\n`);
  return true;
}

async function prepareMetaCliBeforePlatforms(entry, collector) {
  const hasCliActor = entry.scenarios.some(({ value }) => (
    value.cli?.required || value.setup?.initiator === 'cli'
      || (Array.isArray(value.postJoinRecoveryPlan)
        && value.postJoinRecoveryPlan.some(({ sender, approver }) => sender === 'cli' || approver === 'cli'))
  ));
  if (!hasCliActor) return false;

  collector.writeEvent('building meta-cli from current Core sources before platform startup for a declared CLI scenario actor');
  process.stdout.write(`🧭 TEST #${entry.testNumber}: building meta-cli before platform startup\n`);
  await runPreparationCommand('cargo', ['build', '-p', 'meta-cli'], { cwd: metaCliRoot, timeoutMs: 1200000, onOutput: (stream, chunk) => collector.writeOutput(stream, chunk) });
  if (!existsSync(metaCliBinaryPath)) {
    throw new Error(`meta-cli preflight output missing: ${metaCliBinaryPath}`);
  }
  collector.writeEvent('meta-cli built and verified from current Core sources before platform startup');
  process.stdout.write(`🧭 TEST #${entry.testNumber}: meta-cli built and verified before platform startup\n`);
  return true;
}

function parseOptions(args) {
  const options = { from: 1, to: 49, runId: null, preserveEvidence: false, help: false };
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === '--help' || argument === '-h') options.help = true;
    else if (argument === '--preserve-evidence') options.preserveEvidence = true;
    else if (['--from', '--to', '--run-id'].includes(argument)) {
      const value = args[index + 1];
      if (!value || value.startsWith('--')) throw new Error(`${argument} requires a value`);
      index += 1;
      if (argument === '--from') options.from = Number(value);
      else if (argument === '--to') options.to = Number(value);
      else options.runId = value;
    } else if (argument.startsWith('--from=')) options.from = Number(argument.slice('--from='.length));
    else if (argument.startsWith('--to=')) options.to = Number(argument.slice('--to='.length));
    else if (argument.startsWith('--run-id=')) options.runId = argument.slice('--run-id='.length);
    else throw new Error(`Unknown option: ${argument}`);
  }
  if (options.help) return options;
  if (!Number.isInteger(options.from) || !Number.isInteger(options.to)
    || options.from < 1 || options.to > 50 || options.from > options.to) {
    throw new Error('Expected an inclusive test range within 1..50, with --from <= --to');
  }
  if (options.runId && !/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/.test(options.runId)) {
    throw new Error('--run-id may contain only letters, digits, dot, underscore, and hyphen');
  }
  return options;
}

function printHelp() {
  process.stdout.write([
    'Run visible UI E2E scenarios sequentially; pause at a failed test for diagnosis and repair.',
    '',
    'Usage: npm run test-range -- [--from 1] [--to 49] [--run-id ID]',
    'Use --preserve-evidence to retain per-test logs and XCTest artifacts after a green range.',
    'Test #18 is skipped only while its scenario/command remains absent.',
    'Below 15 GiB the runner invokes the allowlisted cleaner automatically.',
    'If cleanup leaves under 12 GiB, it pauses with code 75; free more space,',
    'then resume from the recorded pending test using the same --run-id.',
  ].join('\n') + '\n');
}

function discoverTestScenarios(testNumber) {
  const testDirectory = resolve(e2eRoot, `Test${testNumber}`);
  if (!existsSync(testDirectory)) return [];
  const found = [];
  const visit = (directory, depth = 0) => {
    if (depth > 2) return;
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.name === 'node_modules' || entry.name === '.artifacts' || entry.name === '.derivedData') continue;
      const fullPath = join(directory, entry.name);
      if (entry.isDirectory()) visit(fullPath, depth + 1);
      else if (/^scenario.*\.json$/i.test(entry.name)) {
        found.push({ path: fullPath, value: JSON.parse(readFileSync(fullPath, 'utf8')) });
      }
    }
  };
  visit(testDirectory);
  return found.sort((left, right) => {
    const leftDefault = /(?:^|\/)scenario\.json$/.test(left.path) ? 0 : 1;
    const rightDefault = /(?:^|\/)scenario\.json$/.test(right.path) ? 0 : 1;
    return leftDefault - rightDefault || left.path.localeCompare(right.path);
  });
}

function collectSensitiveValues(value, values, sensitiveContext = false) {
  if (typeof value === 'string') {
    if (sensitiveContext && value.length > 0) values.add(value);
    return;
  }
  if (Array.isArray(value)) {
    for (const entry of value) collectSensitiveValues(entry, values, sensitiveContext);
    return;
  }
  if (!value || typeof value !== 'object') return;
  for (const [key, nested] of Object.entries(value)) {
    const sensitiveKey = /secret|vault|master.?key|private.?key|key.?share|password/i.test(key);
    collectSensitiveValues(nested, values, sensitiveContext || sensitiveKey);
  }
}

function resolveTestCommand(testNumber) {
  const scripts = packageJson.scripts ?? {};
  const blockScript = `test-${testNumber}:blocks`;
  const normalScript = `test-${testNumber}`;
  const script = scripts[blockScript] ? blockScript : (scripts[normalScript] ? normalScript : null);
  if (!script) {
    if (testNumber === 18) return null;
    throw new Error(`Test #${testNumber} has no registered npm script`);
  }
  if (!existsSync(resolve(e2eRoot, `Test${testNumber}`, 'orchestrator.mjs'))) {
    throw new Error(`Test #${testNumber} has a script but no orchestrator.mjs`);
  }
  return script;
}

function resolveRange(options) {
  const entries = [];
  const skippedTests = [];
  for (let testNumber = options.from; testNumber <= options.to; testNumber += 1) {
    const script = resolveTestCommand(testNumber);
    if (!script) {
      skippedTests.push({ test: testNumber, reason: 'No registered test command or orchestrator exists' });
      continue;
    }
    const scenarios = discoverTestScenarios(testNumber);
    if (scenarios.length === 0) throw new Error(`Test #${testNumber} has no scenario JSON for safe announcements and redaction`);
    entries.push({ testNumber, script, scenarios });
  }
  return { entries, skippedTests };
}

function readFreeDiskGiB() {
  const result = spawnSync('df', ['-Pk', e2eRoot], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(`Could not check free disk space: ${result.stderr || result.error?.message}`);
  const rows = result.stdout.trim().split(/\r?\n/);
  const fields = rows.at(-1)?.trim().split(/\s+/) ?? [];
  const availableKiB = Number(fields[3]);
  if (!Number.isFinite(availableKiB)) throw new Error('Could not parse free disk space from df output');
  return Math.round((availableKiB / 1024 / 1024) * 100) / 100;
}

function runDiskCleaner(entry, collector, beforeGiB) {
  const avdName = entry.scenarios[0].value.android?.avdName;
  const cleanerPath = resolve(e2eRoot, 'clean-ui-e2e-space.mjs');
  const args = [cleanerPath];
  if (avdName) args.push(`--avd=${avdName}`);
  collector.writeEvent(`disk space below ${diskThresholdGiB} GiB; invoking allowlisted disk cleaner before TEST #${entry.testNumber}`);
  process.stdout.write(`⏳ Disk space is low; running the approved cleaner before TEST #${entry.testNumber}.\n`);
  const result = spawnSync(process.execPath, args, { cwd: e2eRoot, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`;
  if (output) {
    process.stdout.write(output);
    collector.writeEvent(collector.redactor.redactText(output));
  }
  if (result.error) collector.writeEvent(`disk cleaner could not start: ${result.error.message}`);
  return { status: result.status, error: result.error?.message ?? null, beforeGiB, afterGiB: readFreeDiskGiB() };
}

function gitSnapshot() {
  const head = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: e2eRoot, encoding: 'utf8' });
  const status = spawnSync('git', ['status', '--porcelain'], { cwd: e2eRoot, encoding: 'utf8' });
  return {
    head: head.status === 0 ? head.stdout.trim() : 'unavailable',
    workingTreeDirty: status.status !== 0 || status.stdout.trim().length > 0,
  };
}

function activeParticipants(scenario, redactor) {
  const entries = [
    ['web', 'Web'],
    ['ios', 'iOS'],
    ['android', 'Android'],
    ['cli', 'CLI'],
  ].filter(([key]) => key === 'cli'
    ? (scenario[key] || scenario.setup?.initiator === 'cli' || Array.isArray(scenario.postJoinRecoveryPlan))
    : scenario[key]);
  return entries.map(([key, platformName], index) => {
    let detail = '';
    if (key === 'ios' && scenario.ios.simulatorName) detail = ` (${redactor.redactText(scenario.ios.simulatorName)})`;
    if (key === 'android' && scenario.android.avdName) detail = ` (${redactor.redactText(scenario.android.avdName)})`;
    if (key === 'web') detail = ' (visible browser)';
    return `Device ${String.fromCharCode(65 + index)} — ${platformName}${detail}`;
  });
}

function scenarioSummary(scenario, testNumber) {
  if (typeof scenario.summary === 'string' && scenario.summary.trim()) return scenario.summary.trim();
  const parts = [scenario.name ?? scenario.description ?? `UI scenario ${testNumber}`];
  if (Array.isArray(scenario.postJoinRecoveryPlan)) {
    const roles = scenario.postJoinRecoveryPlan
      .map(({ sender, approver }) => `${sender} → ${approver}`)
      .join('; ');
    parts.push(
      `one pending recovery is declined by device removal; CLI rejoins with a fresh Split; `
      + `three post-join recovery checks are approved and read back (${roles})`,
    );
    return parts.join(' · ');
  }
  const setup = scenario.setup;
  if (setup?.initiator) {
    const joinOrder = Array.isArray(setup.joinOrder) ? setup.joinOrder.join(' → ') : null;
    parts.push(`${setup.initiator} initiates${joinOrder ? `; members join in order ${joinOrder}` : ''}`);
    if (setup.deferredJoinAfterCycle) parts.push(`deferred join after recovery cycle ${setup.deferredJoinAfterCycle}`);
  }
  const groups = scenario.recovery?.groups;
  if (Array.isArray(groups) && groups.length) {
    parts.push(`recovery approvals: ${groups.map((group) => `${group.count}× ${group.approver}`).join(', ')}`);
  } else if (Array.isArray(scenario.recovery?.cyclePlan) && scenario.recovery.cyclePlan.length) {
    const plan = scenario.recovery.cyclePlan;
    const cycles = plan.reduce((sum, item) => sum + (Number(item.count) || 0), 0);
    const actors = new Set();
    for (const item of plan) {
      for (const participant of [...(item.senders ?? []), ...(item.approvals ?? [])]) {
        if (typeof participant === 'string') actors.add(participant);
        else if (participant?.platform) actors.add(participant.platform);
      }
      for (const key of ['firstApprover', 'secondApprover']) {
        const values = Array.isArray(item[key]) ? item[key] : [item[key]];
        for (const value of values) if (typeof value === 'string') actors.add(value);
      }
    }
    const actorList = [...actors].join(', ');
    parts.push(`${cycles} declared plan actions${actorList ? ` across ${actorList}` : ''}`);
  } else if (scenario.recovery?.expectedCycles) {
    parts.push(`${scenario.recovery.expectedRecoveryCycles ?? scenario.recovery.expectedCycles} declared recovery cycles`);
  }
  return parts.join(' · ');
}

function announce(entry, collector) {
  const redactor = collector.redactor;
  const scenario = entry.scenarios[0].value;
  const participants = activeParticipants(scenario, redactor);
  const summary = redactor.redactText(scenarioSummary(scenario, entry.testNumber));
  const announcement = [
    `🚀 TEST #${entry.testNumber} HAS BEEN STARTED!`,
    `📱 Devices: ${participants.join('; ')}`,
    `📖 Scenario: ${summary}`,
  ];
  process.stdout.write(`${announcement.join('\n')}\n`);
  for (const line of announcement) collector.writeEvent(`TEST #${entry.testNumber}: ${line}`);
}

function reportTestStage(entry, collector, message) {
  const line = `🧭 TEST #${entry.testNumber}: ${message}`;
  process.stdout.write(`${line}\n`);
  collector.writeEvent(line);
}

function forwardScenarioProgress(channel, chunk, buffers, entry, collector) {
  const value = buffers[channel] + chunk.toString('utf8');
  const lines = value.split(/\r?\n/);
  buffers[channel] = lines.pop() ?? '';
  for (const rawLine of lines) {
    const progress = scenarioProgressLine(rawLine);
    if (!progress) continue;
    const redacted = collector.redactor.redactText(progress.line).slice(0, 700);
    const visible = progress.kind === 'explicit' ? redacted : `🧪 ${redacted}`;
    process.stdout.write(`${visible}\n`);
    collector.writeEvent(`TEST #${entry.testNumber} progress: ${visible}`);
  }
}

function discoverAttemptArtifacts(testNumber, runId, attempt) {
  const artifactRoot = resolve(e2eRoot, '.artifacts');
  if (!existsSync(artifactRoot)) return [];
  const prefixes = [
    `test-${testNumber}-run-${runId}-attempt-${attempt}`,
    `test-${testNumber}-${runId}-attempt-${attempt}-`,
  ];
  const allowed = /(?:\.xcresult|\.log|\.json|\.txt|\.png)$/i;
  return readdirSync(artifactRoot, { withFileTypes: true })
    .filter((entry) => !entry.isSymbolicLink()
      && prefixes.some((prefix) => entry.name.startsWith(prefix))
      && allowed.test(entry.name))
    .map((entry) => resolve(artifactRoot, entry.name))
    .filter((path) => existsSync(path));
}

function summarizeFailureFromLog(logPath) {
  if (!existsSync(logPath)) return null;
  const lines = readFileSync(logPath, 'utf8').split(/\r?\n/);
  const failurePattern = /XCTAssert\w+ failed|AssertionError|java\.lang\.(?:AssertionError|IllegalStateException)|error:.*(?:failed|timed out)|(?:Error|Exception):.*(?:failed|timed out)|timed out waiting|Timed out waiting/i;
  for (const line of lines.slice(-1000).reverse()) {
    if (!failurePattern.test(line)) continue;
    return line
      .replace(/^\[[^\]]+\]\[(?:stdout|stderr)\]\s*/, '')
      .trim()
      .slice(0, 500);
  }
  return null;
}

function updateReadinessReset(reportPath, resetStatus, details = {}) {
  if (!reportPath || !existsSync(reportPath)) return;
  const report = JSON.parse(readFileSync(reportPath, 'utf8'));
  report.stateReset = { status: resetStatus, ...details };
  report.status = resetStatus === 'passed' ? 'reset-ready' : 'reset-failed';
  report.updatedAt = new Date().toISOString();
  const temporary = `${reportPath}.${process.pid}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(report, null, 2)}\n`, { mode: 0o600, flag: 'wx' });
  renameSync(temporary, reportPath);
}

function createRunId() {
  return new Date().toISOString().replace(/[-:.]/g, '').replace('T', 'T').replace('Z', 'Z')
    + `-${process.pid}`;
}

function writeManifest(path, manifest) {
  const temporary = `${path}.${process.pid}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'w' });
  renameSync(temporary, path);
}

function sendSignalToChild(signal) {
  if (!activeChild) return;
  try {
    if (activeChild.detached && process.platform !== 'win32') process.kill(-activeChild.pid, signal);
    else activeChild.kill(signal);
    if (activeChild.preparation && signal !== 'SIGKILL' && !preparationKillTimer) {
      const ownedChild = activeChild;
      preparationKillTimer = setTimeout(() => {
        if (activeChild !== ownedChild) return;
        try { if (ownedChild.detached && process.platform !== 'win32') process.kill(-ownedChild.pid, 'SIGKILL'); else ownedChild.kill('SIGKILL'); } catch {}
      }, 10000);
      preparationKillTimer.unref();
    }
  } catch (error) {
    if (error.code !== 'ESRCH') process.stderr.write(`Could not signal E2E runner: ${error.message}\n`);
  }
}

process.once('SIGINT', () => {
  interruptedBy = 'SIGINT';
  sendSignalToChild('SIGINT');
});
process.once('SIGTERM', () => {
  interruptedBy = 'SIGTERM';
  sendSignalToChild('SIGTERM');
});

async function runPreparationCommand(command, args, { cwd = coreRoot, allowFailure = false, timeoutMs = 30000, onOutput } = {}) {
  if (interruptedBy) throw new Error(`Preparation interrupted by ${interruptedBy}`);
  const child = spawn(command, args, { cwd, detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
  child.detached = process.platform !== 'win32';
  activeChild = child; child.preparation = true;
  let stdout = ''; let stderr = ''; let timedOut = false;
  child.stdout.on('data', (chunk) => { stdout = `${stdout}${chunk}`.slice(-65536); onOutput?.('stdout', chunk); });
  child.stderr.on('data', (chunk) => { stderr = `${stderr}${chunk}`.slice(-65536); onOutput?.('stderr', chunk); });
  const timer = setTimeout(() => { timedOut = true; sendSignalToChild('SIGTERM'); }, timeoutMs);
  const result = await new Promise((resolveResult, reject) => {
    child.once('error', reject); child.once('close', (exitCode, signal) => resolveResult({ exitCode, signal }));
  }).finally(() => { clearTimeout(timer); clearTimeout(preparationKillTimer); preparationKillTimer = null; activeChild = null; });
  if (timedOut || interruptedBy || (result.exitCode !== 0 && !allowFailure)) throw new Error(`Preparation command ${command} failed (${timedOut ? 'timeout' : interruptedBy ?? result.exitCode ?? result.signal}); raw output omitted`);
  return { ...result, stdout, stderr };
}

async function runEntry(entry, attempt, collector, runId, onRuntimeStarted, preserveEvidence, environmentGuard) {
  const logPath = collector.startTest(entry.testNumber, attempt);
  const startedAt = new Date().toISOString();
  collector.writeEvent(`command: npm run ${entry.script}`);
  const scenario = entry.scenarios[0].value;
  let readiness;
  let androidFfiPrebuilt = false;
  let metaCliPrebuilt = false;
  try {
    await environmentGuard.prepare();
    if (interruptedBy) throw new Error(`Preparation interrupted by ${interruptedBy}`);
    androidFfiPrebuilt = await prepareAndroidFfiBeforePlatforms(entry, collector);
    metaCliPrebuilt = await prepareMetaCliBeforePlatforms(entry, collector);
    if (entry.scenarios.some(({ value }) => value.ios)) {
      await ensureIosFfi({ coreRoot, composeRoot, runAndWait: (command, args, options) => runPreparationCommand(command, args, { ...options, timeoutMs: 2400000, onOutput: (stream, chunk) => collector.writeOutput(stream, chunk) }), evidenceDirectory: resolve(collector.runDirectory, `ios-ffi-test-${entry.testNumber}-attempt-${attempt}`), onEvent: (message) => { collector.writeEvent(message); process.stdout.write(`${message}\n`); } });
    }
    reportTestStage(entry, collector, 'checking and starting the visible platforms required by this scenario');
    readiness = await prepareScenarioPlatforms({
      scenario,
      testNumber: entry.testNumber,
      attempt,
      runId,
      runDirectory: collector.runDirectory,
      onRuntimeStarted,
    });
    collector.writeEvent(`platform readiness report: ${readiness.reportPath}`);
    reportTestStage(entry, collector, 'platforms are ready; resetting local server and app state');
    const reset = await resetE2eScenarioState({
      scenario,
      platforms: readiness.platforms,
      onEvent: (message) => {
        collector.writeEvent(`TEST #${entry.testNumber} state reset: ${message}`);
        process.stdout.write(`🧹 ${message}\n`);
      },
    });
    updateReadinessReset(readiness.reportPath, 'passed', reset);
    collector.writeEvent(`scenario state reset completed: ${JSON.stringify(reset)}`);
    reportTestStage(entry, collector, 'local server and participating app state are clean');
  } catch (error) {
    updateReadinessReset(readiness?.reportPath, 'failed', { error: error.message });
    const finishedAt = new Date().toISOString();
    await collector.finishTest({ status: 'readiness-failed', error: error.message });
    return {
      test: entry.testNumber, script: entry.script, attempt, startedAt, finishedAt,
      status: 'readiness-failed', exitCode: null, signal: null, error: collector.redactor.redactText(error.message),
      logPath, readinessPath: readiness?.reportPath ?? null,
      ownedArtifacts: discoverAttemptArtifacts(entry.testNumber, runId, attempt),
      harnessArtifacts: resolve(e2eRoot, '.artifacts'),
    };
  }
  const child = spawn('npm', ['run', entry.script], {
    cwd: e2eRoot,
    env: {
      ...process.env,
      E2E_EXIT_ON_SUCCESS: '1',
      E2E_RANGE_RUN: '1',
      E2E_RUN_ID: runId,
      E2E_TEST_NUMBER: String(entry.testNumber),
      E2E_TEST_ATTEMPT: String(attempt),
      E2E_READINESS_PATH: readiness.reportPath,
      ...(androidFfiPrebuilt ? { E2E_ANDROID_FFI_PREBUILT: '1' } : {}),
      ...(metaCliPrebuilt ? { E2E_META_CLI_PREBUILT: '1' } : {}),
      ...(preserveEvidence ? { E2E_PRESERVE_EVIDENCE: '1' } : {}),
    },
    detached: process.platform !== 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
    shell: false,
  });
  activeChild = child;
  reportTestStage(entry, collector, 'starting the test scenario; live milestone updates follow');
  const progressBuffers = { stdout: '', stderr: '' };
  let readinessFailure = null;
  const stopMonitor = startPlatformLivenessMonitor({
    platforms: readiness.platforms,
    testNumber: entry.testNumber,
    // Runner-owned runtimes stay booted across tests/blocks. An unplanned
    // device loss is therefore a readiness failure, not an expected reboot.
    unavailableGraceMs: 0,
    onEvent: (message) => collector.writeEvent(message),
    onFailure: (error) => {
      readinessFailure = error;
      try {
        if (child.detached && process.platform !== 'win32') process.kill(-child.pid, 'SIGTERM');
        else child.kill('SIGTERM');
      } catch {}
      setTimeout(() => {
        if (child.exitCode === null && child.signalCode === null) {
          try {
            if (child.detached && process.platform !== 'win32') process.kill(-child.pid, 'SIGKILL');
            else child.kill('SIGKILL');
          } catch {}
        }
      }, 15_000).unref();
    },
  });
  child.stdout.on('data', (chunk) => {
    collector.writeOutput('stdout', chunk);
    forwardScenarioProgress('stdout', chunk, progressBuffers, entry, collector);
  });
  child.stderr.on('data', (chunk) => {
    collector.writeOutput('stderr', chunk);
    forwardScenarioProgress('stderr', chunk, progressBuffers, entry, collector);
  });

  const outcome = await new Promise((resolvePromise) => {
    child.once('error', (error) => resolvePromise({ exitCode: null, signal: null, error }));
    child.once('close', (exitCode, signal) => resolvePromise({ exitCode, signal, error: null }));
  });
  stopMonitor();
  activeChild = null;
  reportTestStage(entry, collector, 'scenario process ended; validating readiness and final result');
  if (!readinessFailure && outcome.exitCode === 0) {
    try {
      const finalReadiness = JSON.parse(readFileSync(readiness.reportPath, 'utf8'));
      if (finalReadiness.status !== 'ready') {
        readinessFailure = new Error('Scenario exited successfully without verifying every participating app and test package before business actions');
      }
    } catch (error) {
      readinessFailure = new Error(`Could not validate platform readiness report: ${error.message}`);
    }
  }
  const status = interruptedBy ? 'interrupted' : readinessFailure ? 'readiness-failed' : (outcome.exitCode === 0 ? 'passed' : 'failed');
  const failureSummary = status === 'failed' ? summarizeFailureFromLog(logPath) : null;
  const errorSummary = readinessFailure?.message ?? outcome.error?.message ?? failureSummary
    ?? (status === 'failed' ? `scenario process exited with code ${outcome.exitCode ?? outcome.signal ?? 'unknown'}; see ${logPath}` : null);
  await collector.finishTest({ status, ...outcome, error: errorSummary });
  return {
    test: entry.testNumber,
    script: entry.script,
    attempt,
    startedAt,
    finishedAt: new Date().toISOString(),
    status,
    exitCode: outcome.exitCode,
    signal: outcome.signal,
    error: errorSummary ? collector.redactor.redactText(errorSummary) : null,
    logPath,
    readinessPath: readiness.reportPath,
    ownedArtifacts: discoverAttemptArtifacts(entry.testNumber, runId, attempt),
    harnessArtifacts: resolve(e2eRoot, '.artifacts'),
  };
}

async function main() {
  const options = parseOptions(process.argv.slice(2));
  if (options.help) {
    printHelp();
    return 0;
  }
  const { entries, skippedTests } = resolveRange(options);
  if (entries.length === 0) throw new Error('The requested range contains no executable tests');
  const runId = options.runId ?? createRunId();
  const runDirectory = resolve(testsRoot, runId);
  mkdirSync(runDirectory, { recursive: true });
  const manifestPath = resolve(runDirectory, 'run.json');
  let manifest = { runId, range: { from: options.from, to: options.to }, status: 'running', tests: [], skippedTests };
  if (existsSync(manifestPath)) {
    manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
    if (manifest.runId !== runId) throw new Error(`Run manifest id does not match ${runId}`);
    if (options.from < manifest.range.from || options.to !== manifest.range.to) {
      throw new Error('A resumed run must keep the original --to and may only move --from forward within the recorded range');
    }
    if (manifest.status === 'completed') throw new Error(`Run ${runId} is already completed`);
    if (!Number.isInteger(manifest.pendingTest) || options.from !== manifest.pendingTest) {
      throw new Error(`Resume must start at the recorded pending test #${manifest.pendingTest ?? 'unknown'}`);
    }
    manifest.status = 'running';
    manifest.pendingTest = null;
    manifest.skippedTests = [...new Map([
      ...(manifest.skippedTests ?? []),
      ...skippedTests,
    ].map((entry) => [entry.test, entry])).values()];
  } else {
    const allScenarioValues = entries.flatMap((entry) => entry.scenarios.map(({ value }) => value));
    const sensitiveValues = new Set();
    for (const scenario of allScenarioValues) collectSensitiveValues(scenario, sensitiveValues);
    const collector = new RunArtifactCollector({ runDirectory, sensitiveValues: [...sensitiveValues] });
    const freeDiskGiB = readFreeDiskGiB();
    manifest.createdAt = new Date().toISOString();
    manifest.artifacts = { directory: runDirectory, runLog: collector.summaryPath, manifest: manifestPath };
    manifest.environment = { platform: platform(), release: release(), arch: arch(), node: process.version, freeDiskGiB, ...gitSnapshot() };
    manifest.tests = [];
    collector.writeEvent(`run ${runId} created; range ${options.from}..${options.to}; initial free disk ${freeDiskGiB} GiB`);
    for (const skipped of skippedTests) {
      collector.writeEvent(`skipped TEST #${skipped.test}: ${skipped.reason}`);
      process.stdout.write(`⏭️ TEST #${skipped.test} is not defined; skipping the absent test.\n`);
    }
    writeManifest(manifestPath, manifest);
    collector.writeEvent(`Run manifest: ${manifestPath}`);
    // Store the collector across the execution loop without serializing it.
    return await executeRange({ entries, options, runId, runDirectory, manifestPath, manifest, collector });
  }

  const allScenarioValues = entries.flatMap((entry) => entry.scenarios.map(({ value }) => value));
  const sensitiveValues = new Set();
  for (const scenario of allScenarioValues) collectSensitiveValues(scenario, sensitiveValues);
  const collector = new RunArtifactCollector({ runDirectory, sensitiveValues: [...sensitiveValues] });
  collector.writeEvent(`resume run ${runId}; pending range starts at test ${options.from}`);
  for (const skipped of skippedTests) {
    collector.writeEvent(`skipped TEST #${skipped.test}: ${skipped.reason}`);
  }
  return await executeRange({ entries, options, runId, runDirectory, manifestPath, manifest, collector });
}

async function executeRange({ entries, options, runId, runDirectory, manifestPath, manifest, collector }) {
  manifest.runtimeOwnership ??= [];
  const allScenarios = entries.flatMap((entry) => entry.scenarios.map(({ value }) => value));
  const environmentGuard = createBatchEnvironmentGuard({
    ...batchEnvironmentPaths(coreRoot, composeRoot), repositories: [coreRoot, composeRoot],
    evidencePath: resolve(runDirectory, `environment-repository-fingerprints-${Date.now()}.json`),
    preflight: () => preflightBatchEnvironment(allScenarios, { runCommand: runPreparationCommand, onEvent: (message) => { collector.writeEvent(message); process.stdout.write(`🧭 ${message}\n`); } }),
    setLocal: () => runPreparationCommand('/opt/homebrew/bin/task', ['set-env', '--', 'local'], { cwd: coreRoot, timeoutMs: 2400000, onOutput: (stream, chunk) => collector.writeOutput(stream, chunk) }),
    isInterrupted: () => Boolean(interruptedBy),
    onEvent: (message) => { collector.writeEvent(`[ENV] ${message}`); process.stdout.write(`🧭 ${message}\n`); },
  });
  try {
    for (const entry of entries) {
      if (interruptedBy) break;
      const completedAttempts = manifest.tests.filter((result) => result.test === entry.testNumber);
      const passedBefore = completedAttempts.some((result) => result.status === 'passed');
      if (passedBefore) continue;
      announce(entry, collector);
      reportTestStage(entry, collector, 'checking available disk space before platform startup');
      const freeDiskGiB = readFreeDiskGiB();
      reportTestStage(entry, collector, `${freeDiskGiB} GiB free; required cleanup threshold is ${diskThresholdGiB} GiB`);
      manifest.diskChecks ??= [];
      manifest.diskChecks.push({ test: entry.testNumber, at: new Date().toISOString(), beforeGiB: freeDiskGiB });
      if (freeDiskGiB < diskThresholdGiB) {
        const cleanup = runDiskCleaner(entry, collector, freeDiskGiB);
        Object.assign(manifest.diskChecks.at(-1), {
          cleanupExitCode: cleanup.status,
          afterGiB: cleanup.afterGiB,
          cleanupError: cleanup.error,
        });
        writeManifest(manifestPath, manifest);
        if (cleanup.status !== 0 || cleanup.afterGiB < postCleanupMinimumGiB) {
          manifest.status = 'paused-low-disk';
          manifest.pendingTest = entry.testNumber;
          manifest.freeDiskGiB = cleanup.afterGiB;
          manifest.cleanupExitCode = cleanup.status;
          manifest.updatedAt = new Date().toISOString();
          writeManifest(manifestPath, manifest);
          collector.writeEvent(`PAUSED before TEST #${entry.testNumber}: free disk after cleaner ${cleanup.afterGiB} GiB; minimum ${postCleanupMinimumGiB} GiB`);
          process.stdout.write(`⏳ 🟡 TEST IS PENDING — Test #${entry.testNumber}: disk cleanup left ${cleanup.afterGiB} GiB free, below the ${postCleanupMinimumGiB} GiB minimum.\n`);
          process.stdout.write(`⏸️ E2E batch paused before Test #${entry.testNumber}: only ${cleanup.afterGiB} GiB is free after cleanup. Free more space, then resume this run.\n`);
          return 75;
        }
        if (cleanup.afterGiB < diskThresholdGiB) {
          collector.writeEvent(`allowing one test after successful allowlisted cleanup at ${cleanup.afterGiB} GiB`);
          process.stdout.write(`✅ Cleanup left ${cleanup.afterGiB} GiB; proceeding with this test, then rechecking space.\n`);
        }
        reportTestStage(entry, collector, `disk cleanup finished; ${cleanup.afterGiB} GiB free`);
      }
      if (passedBefore) continue;
      const attempt = completedAttempts.length + 1;
      const result = await runEntry(entry, attempt, collector, runId, (platform) => {
        const runtime = {
          platform: platform.platform,
          avdName: platform.avdName,
          simulatorName: platform.simulatorName,
          serial: platform.serial,
          udid: platform.udid,
        };
        const identity = runtime.serial ?? runtime.udid;
        if (!manifest.runtimeOwnership.some((owned) => (owned.serial ?? owned.udid) === identity)) {
          manifest.runtimeOwnership.push(runtime);
          writeManifest(manifestPath, manifest);
          collector.writeEvent(`range owns ${runtime.platform} runtime ${runtime.avdName ?? runtime.simulatorName}`);
        }
      }, options.preserveEvidence, environmentGuard);
      result.freeDiskAfterGiB = readFreeDiskGiB();
      manifest.diskChecks.push({ test: entry.testNumber, attempt, at: new Date().toISOString(), afterScenarioGiB: result.freeDiskAfterGiB });
      manifest.tests.push(result);
      manifest.updatedAt = new Date().toISOString();
      if (result.status !== 'passed') {
        manifest.status = interruptedBy ? 'interrupted' : 'failed';
        manifest.failedTest = entry.testNumber;
        manifest.pendingTest = entry.testNumber;
        writeManifest(manifestPath, manifest);
        collector.writeEvent(`paused at TEST #${entry.testNumber}; analyze and repair this test, rerun it successfully, then resume the remaining range with the same run ID`);
        process.stdout.write(interruptedBy
          ? `🏁 TEST RUN INTERRUPTED during Test #${entry.testNumber}.\n`
          : `🏁 ⏳ 🟡 TEST IS PENDING — Test #${entry.testNumber}: ${collector.redactor.redactText(result.error ?? `runner exited ${result.exitCode ?? result.signal ?? 'without a status'}`)}. Evidence preserved; analyzing and repairing before retry, then the range will resume.\n`);
        process.stdout.write(`📁 Run artifacts: ${runDirectory}\n`);
        return result.exitCode && result.exitCode > 0 ? result.exitCode : 1;
      }
      manifest.failedTest = null;
      process.stdout.write(`🏁 🟢 TEST SUCCESS — Test #${entry.testNumber}\n`);
      writeManifest(manifestPath, manifest);
    }

    manifest.status = interruptedBy ? 'interrupted' : 'completed';
    manifest.finishedAt = new Date().toISOString();
    manifest.updatedAt = new Date().toISOString();
    manifest.pendingTest = null;
    manifest.failedTest = null;
    writeManifest(manifestPath, manifest);
    collector.writeEvent(`run ${runId} ${manifest.status}`);
    if (manifest.status === 'completed') {
      if (options.preserveEvidence) {
        manifest.evidencePreserved = true;
        collector.writeEvent('successful-run artifact pruning skipped by --preserve-evidence');
        process.stdout.write('📁 Evidence preserved; successful-run artifact pruning was skipped.\n');
      } else {
        const cleanerPath = resolve(e2eRoot, 'clean-ui-e2e-space.mjs');
        const cleanup = spawnSync(process.execPath, [cleanerPath, '--prune-success-run', runId], {
          cwd: e2eRoot,
          encoding: 'utf8',
          maxBuffer: 4 * 1024 * 1024,
        });
        const output = `${cleanup.stdout ?? ''}${cleanup.stderr ?? ''}`;
        if (output) {
          process.stdout.write(output);
          collector.writeEvent(collector.redactor.redactText(output));
        }
        if (cleanup.status === 0) {
          manifest.rawTestLogsPrunedAt = new Date().toISOString();
          for (const result of manifest.tests) {
            if (result.status === 'passed') result.rawLogRetained = false;
          }
        } else {
          manifest.cleanupWarning = cleanup.error?.message ?? output.trim() ?? `cleaner exited ${cleanup.status}`;
        }
      }
      writeManifest(manifestPath, manifest);
    }
    process.stdout.write(`📁 Run artifacts: ${runDirectory}\n`);
    return interruptedBy ? 130 : 0;
  } finally {
    if (manifest.status !== 'paused-low-disk' && manifest.runtimeOwnership.length > 0) {
      try {
        await stopRunnerOwnedRuntimes(manifest.runtimeOwnership, (message) => {
          collector.writeEvent(message);
          process.stdout.write(`🧹 ${message}\n`);
        });
        manifest.runtimeOwnership = [];
      } catch (error) {
        manifest.runtimeCleanupWarning = error.message;
        collector.writeEvent(`runtime cleanup warning: ${error.message}`);
        process.stderr.write(`⚠️ Runtime cleanup warning: ${error.message}\n`);
      }
      manifest.updatedAt = new Date().toISOString();
      writeManifest(manifestPath, manifest);
    }
    try {
      await environmentGuard.restore();
      manifest.environmentRestoration = { status: 'passed', at: new Date().toISOString() };
    } catch (error) {
      manifest.environmentRestoration = { status: 'failed', error: error.message };
      manifest.status = 'restore-failed';
      throw error;
    } finally { writeManifest(manifestPath, manifest); await collector.close(); }
  }
}

main().then((code) => {
  process.exitCode = code;
}).catch((error) => {
  process.stderr.write(`E2E range runner failed before/around a test: ${error.stack ?? error.message}\n`);
  process.exitCode = 1;
});
