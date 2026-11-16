import { assertIosIdentityAdmission, createIosIdentityContext, prepareIosIdentityBoundary } from './shared/ios-test-identity.mjs';
import { buildDeletionPlan, resolveDeletionAuthProfile, validateDeletionResult, requireDeletionResumeProfile, deletionEntryComplete } from './Test50/plan.mjs';
import { capturePreparationCommand } from './shared/preparation-command.mjs';
import { E2E_BUILD_PROFILE, e2eBuildEnvironment, measurePreparationInputs, planPreparationBudget, createPreparationResourceBudget, readPreparationObservation } from './shared/e2e-resource-budget.mjs';
import { createPreparationMetrics } from './shared/preparation-metrics.mjs';
import { createRunnerAudit } from './shared/runner-validation-audit.mjs';
import { validateRunnerAudit, verifyRunnerAuditLineage } from './shared/batch-replay-identity.mjs';
import { createScenarioRuntimeProfile, assertScenarioReadiness } from './shared/scenario-runtime-profile.mjs';
import { createBatchProductPreparation } from './shared/batch-product-builders.mjs';
import { parseUiE2eRangeOptions, selectUiE2eCommand } from './shared/ui-e2e-range-selection.mjs';
import { scenarioProgressLine } from './shared/scenario-progress.mjs';
import { batchEnvironmentPaths, createBatchEnvironmentGuard, preflightBatchEnvironment } from './shared/batch-environment.mjs';
import { spawn, spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { arch, platform, release, tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { ensureIosFfi } from './shared/ios-ffi-provenance.mjs';
import { createDiagnosticRedactor } from './shared/diagnostic-redaction.mjs';
import {
  prepareScenarioPlatforms,
  startPlatformLivenessMonitor,
  stopRunnerOwnedRuntimes,
  releaseHealthyRunnerOwnedRuntimes,
  assertExistingDeviceRun,
} from './shared/platform-supervisor.mjs';
import { discoverAndroidSdk } from './shared/android-sdk.mjs';
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
let metaCliSourcePrepared;

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
  await runPreparationCommand('bash', [androidFfiBuildScript], { cwd: coreRoot, env: { CARGO_BUILD_JOBS: '2' }, timeoutMs: 2400000, onOutput: (stream, chunk) => collector.writeOutput(stream, chunk) });

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

  const source = fingerprintAndroidCoreSource();
  if (metaCliSourcePrepared === source && existsSync(metaCliBinaryPath)) return true;
  collector.writeEvent('building meta-cli from current Core sources before platform startup for a declared CLI scenario actor');
  process.stdout.write(`🧭 TEST #${entry.testNumber}: building meta-cli before platform startup\n`);
  await runPreparationCommand('cargo', ['build', '-p', 'meta-cli'], { cwd: metaCliRoot, env: { CARGO_BUILD_JOBS: '2' }, timeoutMs: 1200000, onOutput: (stream, chunk) => collector.writeOutput(stream, chunk) });
  if (!existsSync(metaCliBinaryPath)) {
    throw new Error(`meta-cli preflight output missing: ${metaCliBinaryPath}`);
  }
  metaCliSourcePrepared = source;
  collector.writeEvent('meta-cli built and verified from current Core sources before platform startup');
  process.stdout.write(`🧭 TEST #${entry.testNumber}: meta-cli built and verified before platform startup\n`);
  return true;
}

function parseOptions(args) {
  return parseUiE2eRangeOptions(args);
}

function printHelp() {
  process.stdout.write([
    'Run visible UI E2E scenarios sequentially; pause at a failed test for diagnosis and repair.',
    '',
    'Usage: npm run test-range -- [--from 1] [--to 50] [--run-id ID]',
    'Diagnostic only: --from 9 --to 9 OR --from 11 --to 11, with --diagnostic-block 2 --run-id FRESH_ID (no whole-test/cursor credit).',
    'Use --preserve-evidence to retain per-test logs and XCTest artifacts after a green range.',
    'Optional --pause-after-each-case checkpoints after each whole PASS (exit 75); resume the same run ID from its recorded pending test with the original --to.',
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

function resolveTestCommand(testNumber, diagnosticBlock) {
  return selectUiE2eCommand(testNumber, packageJson.scripts ?? {}, existsSync(resolve(e2eRoot, `Test${testNumber}`, 'orchestrator.mjs')), diagnosticBlock);
}

function resolveRange(options) {
  const entries = [];
  const skippedTests = [];
  for (let testNumber = options.from; testNumber <= options.to; testNumber += 1) {
    const script = resolveTestCommand(testNumber, options.diagnosticBlock);
    if (!script) {
      skippedTests.push({ test: testNumber, reason: 'No registered test command or orchestrator exists' });
      continue;
    }
    let scenarios = discoverTestScenarios(testNumber);
    if (options.diagnosticBlock !== undefined) {
      const selectedPath = resolve(e2eRoot, `Test${testNumber}`, `scenario-block${options.diagnosticBlock}.json`);
      scenarios = scenarios.filter(scenario => scenario.path === selectedPath);
      if (scenarios.length !== 1) throw new Error('Diagnostic block requires exactly its original canonical scenario');
    }
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

function runDiskCleaner(entry, collector, beforeGiB, preservePreparedBuilds = false) {
  const cleanerPath = resolve(e2eRoot, 'clean-ui-e2e-space.mjs');
  const args = [cleanerPath, ...(preservePreparedBuilds ? ['--preserve-prepared-builds'] : [])];
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
  const runtimeProfile = createScenarioRuntimeProfile({ testNumber: entry.testNumber, scenario });
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

export async function runPreparationCommand(command, args, { cwd = coreRoot, allowFailure = false, timeoutMs = 30000, onOutput, env = {}, maxCaptureBytes, ports = {} } = {}) {
  return capturePreparationCommand(command, args, { cwd, allowFailure, timeoutMs, onOutput, env: { ...process.env, ...env }, maxCaptureBytes }, {
    spawn: ports.spawn ?? spawn,
    signal: ports.signal ?? ((_child, signal) => sendSignalToChild(signal)),
    interrupted: () => interruptedBy,
    started: (child) => { activeChild = child; },
    settled: () => { clearTimeout(preparationKillTimer); preparationKillTimer = null; activeChild = null; },
  });
}
const rangeExecutionServices = { spawn, prepareScenarioPlatforms, resetE2eScenarioState, startPlatformLivenessMonitor, discoverAttemptArtifacts };

export function assertAcceptedRunnerLineage({ paths, runId, manifest, currentIdentity }, { readAudit = verifyRunnerAuditLineage } = {}) {
  const executionIdentity = (identity, testNumber) => {
    const noncompiled = Object.fromEntries(Object.entries(identity.noncompiled).filter(([path]) =>
      path !== 'e2eTest/README.md' && !path.endsWith('.test.mjs')
      && (!/^e2eTest\/Test[1-3]\/orchestrator\.mjs$/.test(path) || path === `e2eTest/Test${testNumber}/orchestrator.mjs`)));
    return { membership: identity.membership, strict: identity.strict, scenario: identity.scenario, noncompiled };
  };
  for (const result of manifest.tests) {
    if (!['passed', 'business-passed-manual-pending'].includes(result.status)) continue;
    const previous = readAudit({ paths, runId, audit: result.runnerAudit });
    if (JSON.stringify(executionIdentity(previous.before, result.test)) !== JSON.stringify(executionIdentity(currentIdentity, result.test))) {
      throw Error(`Accepted Test #${result.test} execution inputs changed; previous evidence preserved, reviewed range disposition required`);
    }
  }
}

function expectedDeletionBlocks(runId, authProfile) {
  return buildDeletionPlan(runId).blocks.map(block => {
    const path = resolve(e2eRoot, '.artifacts', runId, 'test50-config', `scenario-${block.id}.json`);
    const raw = readFileSync(path), configuration = JSON.parse(raw);
    if (configuration.authProfile !== authProfile || JSON.stringify(configuration.deletionBlock) !== JSON.stringify(block)) throw Error('Test50 generated configuration changed');
    return { id: block.id, cycles: block.cycles.map(cycle => cycle.number), configurationSha256: sha256(raw) };
  });
}

function completedRangeEntry(result, authProfile, runId) {
  if (result.scope?.kind === 'diagnostic-block' || result.fullAcceptance === false && result.diagnosticBlock !== undefined) return false;
  if (!deletionEntryComplete(result, authProfile)) {
    if (result.test === 50 && result.status === 'business-passed-manual-pending') throw Error('Invalid stored Test50 business acceptance');
    return false;
  }
  if (result.test !== 50) return true;
  // Old unprofiled realOS records keep their original meaning; scoped records need their exact receipt.
  if (authProfile === 'real-os' && !result.authProfile) return true;
  const path = resolve(e2eRoot, '.artifacts', runId, 'test50-config', `result-attempt-${result.attempt}.json`);
  const raw = readFileSync(path);
  if (sha256(raw) !== result.acceptanceSha256) throw Error('Test50 resume receipt changed');
  validateDeletionResult(JSON.parse(raw), { runId, attempt: result.attempt, authProfile, blocks: expectedDeletionBlocks(runId, authProfile) });
  return true;
}

export async function runEntry(entry, run, preparation, services = {}) {
  const { attempt, collector, runId, onRuntimeStarted, preserveEvidence } = run;
  const { environmentGuard, batchProducts, getIosProfile, getAndroidProfile, resourceBudget, preparationMetrics, runnerAudit } = preparation;
  const measure = (phase, operation) => preparationMetrics ? preparationMetrics.measure(phase, operation) : operation();
  const { spawn, prepareScenarioPlatforms, resetE2eScenarioState, startPlatformLivenessMonitor, discoverAttemptArtifacts } = { ...rangeExecutionServices, ...services };
  const scenario = entry.scenarios[0].value;
  const authProfile = entry.testNumber === 50 ? resolveDeletionAuthProfile(process.env, {
    ...scenario, ...(run.test50AuthProfile !== undefined ? { authProfile: run.test50AuthProfile } : {}),
  }) : undefined;
  const keepEvidence = preserveEvidence || authProfile === 'controlled-business' || run.diagnosticScope?.kind === 'diagnostic-block';
  assertExistingDeviceRun({ scenarios: [scenario] });
  const logPath = collector.startTest(entry.testNumber, attempt);
  const startedAt = new Date().toISOString();
  collector.writeEvent(`command: npm run ${entry.script}`);
  assertIosIdentityAdmission({ testNumber: entry.testNumber, scenario });
  const iosIdentityContexts = entry.scenarios.map(({ path, value }) => value.ios
    ? createIosIdentityContext({ runRoot: collector.runDirectory, runId,
      testNumber: entry.testNumber, attempt, scenarioPath: path,
      scenario: value, udid: 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30' }) : undefined);
  for (const context of iosIdentityContexts) {
    if (context) collector.redactor.addSensitiveValues([prepareIosIdentityBoundary(context).namespace]);
  }
  const iosIdentityContext = iosIdentityContexts[0];
  const runtimeProfile = createScenarioRuntimeProfile({ testNumber: entry.testNumber, scenario });
  let readiness;
  let androidFfiPrebuilt = false;
  let metaCliPrebuilt = false;
  let productIndexPath;
  try {
    await resourceBudget?.check();
    await measure('snapshot', () => environmentGuard.prepare());
    if (resourceBudget && !resourceBudget.snapshot?.().snapshot) resourceBudget.complete('snapshot');
    if (interruptedBy) throw new Error(`Preparation interrupted by ${interruptedBy}`);
    productIndexPath = await batchProducts.prepare();
    androidFfiPrebuilt ||= batchProducts.nativePreparation()?.android ?? false;
    metaCliPrebuilt ||= batchProducts.nativePreparation()?.cli ?? false;
    reportTestStage(entry, collector, 'checking and starting the visible platforms required by this scenario');
    const ownedIosProfile = scenario.ios && getIosProfile ? await getIosProfile(scenario.ios.simulatorName) : undefined;
    const ownedAndroidLease = scenario.android && getAndroidProfile ? await getAndroidProfile(scenario.android.avdName) : undefined;
    await resourceBudget?.check();
    readiness = await measure('platform-readiness', () => prepareScenarioPlatforms({
      scenario,
      runtimeProfile,
      testNumber: entry.testNumber,
      attempt,
      runId,
      runDirectory: collector.runDirectory,
      onRuntimeStarted,
      onDiagnostic: (line) => { collector.writeEvent(line); process.stdout.write(`${line}\n`); },
      ownedIosProfile,
      ownedAndroidLease,
    }));
    assertExistingDeviceRun({ scenarios: [scenario], readiness });
    collector.writeEvent(`platform readiness report: ${readiness.reportPath}`);
    reportTestStage(entry, collector, 'platforms are ready; resetting local server and app state');
    const reset = runtimeProfile.kind === 'core-http' ? { serverReset: false, platformsReset: [], ownership: 'child-owned Test20 isolated SQLite + actual meta-server' } : await measure('state-reset', () => resetE2eScenarioState({
      scenario,
      platforms: readiness.platforms,
      runId,
      readinessPath: readiness.reportPath,
      iosIdentityContext,
      onEvent: (message) => {
        collector.writeEvent(`TEST #${entry.testNumber} state reset: ${message}`);
        process.stdout.write(`🧹 ${message}\n`);
      },
    }));
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
      ...(runnerAudit ? { runnerAudit } : {}),
      ownedArtifacts: discoverAttemptArtifacts(entry.testNumber, runId, attempt),
      harnessArtifacts: resolve(e2eRoot, '.artifacts'),
    };
  }
  let readinessFailure = null;
  const outcome = await measure('business-scenario', async () => {
    const child = spawn('npm', ['run', entry.script], {
      cwd: e2eRoot,
      env: {
        ...process.env,
        E2E_EXIT_ON_SUCCESS: '1',
        E2E_RANGE_RUN: '1',
        E2E_RUN_ID: runId,
        E2E_TEST_NUMBER: String(entry.testNumber),
        E2E_TEST_ATTEMPT: String(attempt),
        ...(runnerAudit ? { E2E_PARENT_RUNNER_PID: String(process.pid), E2E_RUNNER_AUDIT_PATH: runnerAudit.path, E2E_RUNNER_AUDIT_SHA256: runnerAudit.sha256 } : {}),
        ...(authProfile ? { E2E_TEST50_AUTH_PROFILE: authProfile } : {}),
        E2E_READINESS_PATH: readiness.reportPath,
        ...(iosIdentityContext ? { E2E_IOS_IDENTITY_ROOT: iosIdentityContext.runRoot } : {}),
        ...(readiness.platforms.find(platform => platform.ownedAndroidProfile)?.ownedAndroidProfile.privateEnv
          ? { E2E_ANDROID_OWNED_PROFILE: '1', ...Object.fromEntries(Object.entries(readiness.platforms.find(platform => platform.ownedAndroidProfile).ownedAndroidProfile.privateEnv)
            .filter(([key]) => key.startsWith('ANDROID_') || key === 'XDG_CACHE_HOME')) } : {}),
        ...(runtimeProfile.kind === 'ui' ? { E2E_BATCH_PRODUCTS_PATH: productIndexPath, E2E_REUSE_PROVEN_ARTIFACTS: '1' } : {}),
        ...(androidFfiPrebuilt ? { E2E_ANDROID_FFI_PREBUILT: '1' } : {}),
        ...(metaCliPrebuilt ? { E2E_META_CLI_PREBUILT: '1' } : {}),
        ...(keepEvidence ? { E2E_PRESERVE_EVIDENCE: '1' } : {}),
      },
      detached: process.platform !== 'win32',
      stdio: ['ignore', 'pipe', 'pipe'],
      shell: false,
    });
    activeChild = child;
    reportTestStage(entry, collector, 'starting the test scenario; live milestone updates follow');
    const progressBuffers = { stdout: '', stderr: '' };
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
      let actorError;
      child.once('error', (error) => { actorError = error; });
      child.once('close', (exitCode, signal) => resolvePromise({ exitCode, signal, error: actorError ?? null }));
    });
    stopMonitor();
    activeChild = null;
    return outcome;
  });
  reportTestStage(entry, collector, 'scenario process ended; validating readiness and final result');
  if (!readinessFailure && outcome.exitCode === 0 && outcome.signal === null && !outcome.error) {
    try {
      const finalReadiness = JSON.parse(readFileSync(readiness.reportPath, 'utf8'));
      assertScenarioReadiness({ profile: runtimeProfile, report: finalReadiness, outcome });
    } catch (error) {
      readinessFailure = new Error(`Could not validate platform readiness report: ${error.message}`);
    }
  }
  let acceptance, acceptanceFailure;
  const acceptancePath = authProfile ? resolve(e2eRoot, '.artifacts', runId, 'test50-config', `result-attempt-${attempt}.json`) : undefined;
  if (authProfile && !readinessFailure && outcome.exitCode === 0 && outcome.signal === null) {
    try {
      acceptance = validateDeletionResult(JSON.parse(readFileSync(acceptancePath, 'utf8')), {
        runId, attempt, authProfile, blocks: expectedDeletionBlocks(runId, authProfile),
      });
    } catch { acceptanceFailure = Error('Test50 full profile receipt is missing, partial or mismatched'); }
  }
  const status = interruptedBy ? 'interrupted' : readinessFailure ? 'readiness-failed' : acceptanceFailure || outcome.error ? 'failed'
    : (outcome.exitCode === 0 && outcome.signal === null ? authProfile === 'controlled-business' ? 'business-passed-manual-pending' : 'passed' : 'failed');
  const failureSummary = status === 'failed' ? summarizeFailureFromLog(logPath) : null;
  const errorSummary = readinessFailure?.message ?? acceptanceFailure?.message ?? outcome.error?.message ?? failureSummary
    ?? (status === 'failed' ? `scenario process exited with code ${outcome.exitCode ?? outcome.signal ?? 'unknown'}; see ${logPath}` : null);
  await collector.finishTest({ status: run.diagnosticScope ? (status === 'passed' ? 'diagnostic-completed' : 'diagnostic-failed') : status, ...outcome, error: errorSummary });
  return {
    test: entry.testNumber,
    script: entry.script,
    attempt,
    startedAt,
    finishedAt: new Date().toISOString(),
    status,
    ...(authProfile ? { authProfile, acceptance, acceptancePath, ...(acceptance ? { acceptanceSha256: sha256(readFileSync(acceptancePath)) } : {}) } : {}),
    exitCode: outcome.exitCode,
    signal: outcome.signal,
    error: errorSummary ? collector.redactor.redactText(errorSummary) : null,
    logPath,
    readinessPath: readiness.reportPath,
    ...(runnerAudit ? { runnerAudit } : {}),
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
  const test50AuthProfile = entries.some(entry => entry.testNumber === 50) ? resolveDeletionAuthProfile(process.env) : undefined;
  if (entries.length === 0) throw new Error('The requested range contains no executable tests');
  const runId = options.runId ?? createRunId();
  const runDirectory = resolve(testsRoot, runId);
  if (options.diagnosticBlock !== undefined) {
    mkdirSync(testsRoot, { recursive: true });
    // Atomic exclusive reservation: every pre-existing path (including symlinks) is rejected.
    mkdirSync(runDirectory);
  } else mkdirSync(runDirectory, { recursive: true });
  const manifestPath = resolve(runDirectory, 'run.json');
  let manifest = { runId, range: { from: options.from, to: options.to }, status: 'running', tests: [], skippedTests };
  if (options.diagnosticBlock !== undefined) {
    manifest.scope = { kind: 'diagnostic-block', test: options.from, block: options.diagnosticBlock,
      scenarioPath: entries[0].scenarios[0].path, scenarioSha256: sha256(readFileSync(entries[0].scenarios[0].path)), script: entries[0].script };
    manifest.fullAcceptance = false; manifest.diagnostics = []; manifest.evidencePreserved = true;
  }
  if (test50AuthProfile) manifest.test50AuthProfile = test50AuthProfile;
  if (existsSync(manifestPath)) {
    const stored = JSON.parse(readFileSync(manifestPath, 'utf8'));
    if (options.diagnosticBlock !== undefined || stored.scope?.kind === 'diagnostic-block') throw new Error('Diagnostic block requires a fresh run ID and cannot grant range resume credit');
    manifest = stored;
    if (test50AuthProfile) { requireDeletionResumeProfile(manifest.test50AuthProfile, test50AuthProfile); manifest.test50AuthProfile = test50AuthProfile; }
    if (manifest.runId !== runId) throw new Error(`Run manifest id does not match ${runId}`);
    if (options.from < manifest.range.from || options.to !== manifest.range.to) {
      throw new Error('A resumed run must keep the original --to and may only move --from forward within the recorded range');
    }
    if (['completed', 'business-completed-manual-pending'].includes(manifest.status)) throw new Error(`Run ${runId} is already completed in its declared scope`);
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

async function finishRangeOwnership({ manifest, manifestPath, collector, environmentGuard, strictDeletion, primaryFailure, retainHealthyOnPause = false, services = {} }) {
  const failures = [];
  const stop = services.stopRunnerOwnedRuntimes ?? stopRunnerOwnedRuntimes;
  const release = services.releaseHealthyRunnerOwnedRuntimes ?? releaseHealthyRunnerOwnedRuntimes;
  const pendingResult = manifest.tests?.at(-1);
  const technicalPause = manifest.status === 'paused-maintenance' || manifest.status === 'paused-low-disk'
    || pendingResult?.test === manifest.pendingTest && pendingResult?.status === 'readiness-failed';
  if (manifest.runtimeOwnership.length > 0) {
    try {
      const onEvent = (message) => {
        collector.writeEvent(message); process.stdout.write(`🧹 ${message}\n`);
      };
      if (retainHealthyOnPause && !primaryFailure && technicalPause && Number.isInteger(manifest.pendingTest)
        && ['failed', 'paused-low-disk', 'paused-maintenance'].includes(manifest.status)) {
        try {
          const receipts = await release(manifest.runtimeOwnership, { reason: 'pending-test', onEvent });
          manifest.runtimeReleases ??= [];
          manifest.runtimeReleases.push(...receipts);
        } catch {
          manifest.runtimeRetentionWarning = 'Healthy runtime release unavailable; applied existing owned cleanup';
          collector.writeEvent(manifest.runtimeRetentionWarning);
          await stop(manifest.runtimeOwnership, onEvent);
        }
      } else await stop(manifest.runtimeOwnership, onEvent);
      manifest.runtimeOwnership = [];
    } catch (error) {
      manifest.runtimeCleanupWarning = error.message;
      if (strictDeletion || manifest.scope?.kind === 'diagnostic-block' || manifest.runtimeOwnership.some(runtime => runtime.ownedIosProfile || runtime.ownedAndroidLease || runtime.ownedAndroidProfile)) {
        failures.push(error); manifest.status = 'owned-runtime-cleanup-failed';
        manifest.evidencePreserved = true; manifest.fullAcceptance = false;
      }
      collector.writeEvent(`runtime cleanup warning: ${error.message}`);
      process.stderr.write(`⚠️ Runtime cleanup warning: ${error.message}\n`);
    }
    manifest.updatedAt = new Date().toISOString();
  }
  try {
    await environmentGuard.restore();
    manifest.environmentRestoration = { status: 'passed', at: new Date().toISOString() };
  } catch (error) {
    manifest.environmentRestoration = { status: 'failed', error: error.message };
    manifest.status = 'restore-failed'; failures.push(error);
  }
  try { writeManifest(manifestPath, manifest); } catch (error) { failures.push(error); }
  try { await collector.close(); } catch (error) { failures.push(error); }
  if (failures.length) {
    if (!primaryFailure && failures.length === 1) throw failures[0];
    throw new AggregateError([primaryFailure, ...failures].filter(Boolean), 'Range execution/owned cleanup failed', { cause: primaryFailure ?? failures[0] });
  }
}

async function executeRange({ entries, options, runId, runDirectory, manifestPath, manifest, collector }) {
  const strictDeletion = entries.some(entry => entry.testNumber === 50) || manifest.tests.some(result => result.test === 50);
  let primaryFailure;
  if (manifest.runtimeOwnership === undefined) manifest.runtimeOwnership = [];
  const registeredScenarios = entries.flatMap((entry) => entry.scenarios.map(({ value }) => ({ testNumber: entry.testNumber, scenario: value })));
  const allScenarios = registeredScenarios.map(({ scenario }) => scenario);
  // Refuse legacy creation/resume before guard preparation and before profile-cleaning finally.
  assertExistingDeviceRun({ scenarios: allScenarios, manifest });
  const runtimeProfiles = registeredScenarios.map(createScenarioRuntimeProfile);
  const resourceInputs = measurePreparationInputs({ paths: { coreRoot, composeRoot, runDirectory }, scenarios: registeredScenarios });
  const budget = planPreparationBudget(resourceInputs);
  const observe = () => readPreparationObservation({ path: runDirectory, runAndCapture: runPreparationCommand });
  const resourceBudget = createPreparationResourceBudget({ budget, observe });
  const metricPath = resolve(runDirectory, `phase-metrics-${Date.now()}.jsonl`);
  const preparationMetrics = createPreparationMetrics({ observe, write: record => writeFileSync(metricPath, `${JSON.stringify(record)}\n`, { flag: 'a', mode: 0o600 }) });
  manifest.preparationBudget = budget;
  manifest.preparationProfile = { ...E2E_BUILD_PROFILE, cargoJobs: 2 };
  const runnerAudit = await preparationMetrics.measure('f1-validation', () => createRunnerAudit({
    paths: { coreRoot, composeRoot, runDirectory }, runId, runAndCapture: runPreparationCommand,
    redactText: value => collector.redactor.redactText(value),
  }));
  manifest.runnerAudits ??= [];
  manifest.runnerAudits.push(runnerAudit);
  writeManifest(manifestPath, manifest);
  try {
    assertAcceptedRunnerLineage({ paths: { coreRoot, composeRoot, runDirectory }, runId, manifest,
      currentIdentity: validateRunnerAudit({ paths: { coreRoot, composeRoot, runDirectory }, runId, audit: runnerAudit }) });
  } catch (error) {
    manifest.status = 'runner-lineage-review-required';
    manifest.pendingTest = options.diagnosticBlock !== undefined ? null : entries[0].testNumber;
    manifest.runnerLineageError = collector.redactor.redactText(error.message);
    writeManifest(manifestPath, manifest);
    throw error;
  }
  const environmentGuard = createBatchEnvironmentGuard({
    runDirectory,
    ...batchEnvironmentPaths(coreRoot, composeRoot), repositories: [coreRoot, composeRoot],
    evidencePath: resolve(runDirectory, `environment-repository-fingerprints-${Date.now()}.json`),
    preflight: () => preflightBatchEnvironment(allScenarios, { runtimeProfiles, ownedIosProfile: false, runCommand: runPreparationCommand, onEvent: (message) => { collector.writeEvent(message); process.stdout.write(`🧭 ${message}\n`); } }),
    setLocal: () => runPreparationCommand('/opt/homebrew/bin/task', ['set-env', '--', 'local', '--selectors-only'], { cwd: coreRoot, timeoutMs: 2400000, onOutput: (stream, chunk) => collector.writeOutput(stream, chunk) }),
    isInterrupted: () => Boolean(interruptedBy),
    onEvent: (message) => { collector.writeEvent(`[ENV] ${message}`); process.stdout.write(`🧭 ${message}\n`); },
  });
  const batchProducts = createBatchProductPreparation({ paths: { coreRoot, composeRoot, runDirectory },
    commands: { runAndCapture: runPreparationCommand, runAndWait: (file, args, options) => runPreparationCommand(file, args, { ...options, timeoutMs: 2400000, onOutput: (stream, chunk) => collector.writeOutput(stream, chunk) }) },
    runId, scenarios: registeredScenarios, environmentGuard, resourceBudget, preparationMetrics, newProofVersion: 5, runnerAudit,
    prepareNative: async () => {
      const combined = { testNumber: entries[0].testNumber, scenarios: registeredScenarios.map(({ scenario: value }) => ({ value })) };
      const android = await prepareAndroidFfiBeforePlatforms(combined, collector);
      const cli = await prepareMetaCliBeforePlatforms(combined, collector);
      if (registeredScenarios.some(({ scenario }) => scenario.ios)) await ensureIosFfi({ coreRoot, composeRoot,
        runAndWait: (file, args, options) => runPreparationCommand(file, args, { ...options, env: { ...e2eBuildEnvironment(), CARGO_BUILD_JOBS: '2', ...options?.env }, timeoutMs: 2400000 }), evidenceDirectory: resolve(runDirectory, 'ios-ffi-batch') });
      return { android, cli, ios: registeredScenarios.some(({ scenario }) => scenario.ios) };
    },
  });
  try {
    for (const entry of entries) {
      if (interruptedBy) break;
      const completedAttempts = manifest.tests.filter((result) => result.test === entry.testNumber);
      const passedBefore = completedAttempts.some((result) => completedRangeEntry(result, manifest.test50AuthProfile ?? 'real-os', runId));
      if (passedBefore) continue;
      announce(entry, collector);
      reportTestStage(entry, collector, 'checking available disk space before platform startup');
      const freeDiskGiB = readFreeDiskGiB();
      reportTestStage(entry, collector, `${freeDiskGiB} GiB free; required cleanup threshold is ${diskThresholdGiB} GiB`);
      manifest.diskChecks ??= [];
      manifest.diskChecks.push({ test: entry.testNumber, at: new Date().toISOString(), beforeGiB: freeDiskGiB });
      if (freeDiskGiB < diskThresholdGiB) {
        const cleanup = runDiskCleaner(entry, collector, freeDiskGiB,
          batchProducts.hasPersistedProof || Boolean(batchProducts.nativePreparation()));
        Object.assign(manifest.diskChecks.at(-1), {
          cleanupExitCode: cleanup.status,
          afterGiB: cleanup.afterGiB,
          cleanupError: cleanup.error,
        });
        writeManifest(manifestPath, manifest);
        if (cleanup.status !== 0 || cleanup.afterGiB < postCleanupMinimumGiB) {
          manifest.status = options.diagnosticBlock !== undefined ? 'diagnostic-blocked-low-disk' : 'paused-low-disk';
          manifest.pendingTest = options.diagnosticBlock !== undefined ? null : entry.testNumber;
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
      const result = await runEntry(entry, { attempt, collector, runId, onRuntimeStarted: (platform) => {
        const runtime = platform;
        if (platform.ownedAndroidLease) {
          const own = manifest.runtimeOwnership.find(item => item.ownedAndroidLease === platform.ownedAndroidLease);
          if (!own) throw new Error('Android profile intent missing before runtime publication');
          Object.assign(own, runtime); writeManifest(manifestPath, manifest); return;
        }
        const identity = runtime.serial ?? runtime.udid;
        if (!manifest.runtimeOwnership.some((owned) => (owned.serial ?? owned.udid) === identity)) {
          manifest.runtimeOwnership.push(runtime);
          writeManifest(manifestPath, manifest);
          collector.writeEvent(`range owns ${runtime.platform} runtime ${runtime.avdName ?? runtime.simulatorName}`);
        }
      }, preserveEvidence: options.preserveEvidence || options.diagnosticBlock !== undefined, test50AuthProfile: manifest.test50AuthProfile, diagnosticScope: manifest.scope?.kind === 'diagnostic-block' ? manifest.scope : undefined }, { environmentGuard, batchProducts, resourceBudget, preparationMetrics, runnerAudit });
      result.freeDiskAfterGiB = readFreeDiskGiB();
      manifest.diskChecks.push({ test: entry.testNumber, attempt, at: new Date().toISOString(), afterScenarioGiB: result.freeDiskAfterGiB });
      if (options.diagnosticBlock !== undefined) {
        const succeeded = result.status === 'passed' && result.exitCode === 0 && result.signal === null;
        manifest.diagnostics.push({ ...result, scope: manifest.scope, originalActorStatus: result.status,
          status: succeeded ? 'diagnostic-completed' : 'diagnostic-failed', fullAcceptance: false });
        manifest.status = interruptedBy ? 'diagnostic-interrupted' : succeeded ? 'diagnostic-completed' : 'diagnostic-failed';
        manifest.pendingTest = null; manifest.failedTest = null;
        manifest.finishedAt = new Date().toISOString(); manifest.updatedAt = manifest.finishedAt;
        writeManifest(manifestPath, manifest);
        collector.writeEvent(`BLOCK DIAGNOSTIC ${manifest.status}; whole Test #${entry.testNumber} UNRUN; no cursor credit`);
        process.stdout.write(`🔎 BLOCK DIAGNOSTIC ${manifest.status}; whole Test #${entry.testNumber} UNRUN; fullAcceptance=false\n`);
        return interruptedBy ? 130 : succeeded ? 0 : result.exitCode > 0 ? result.exitCode : 1;
      }
      manifest.tests.push(result);
      manifest.updatedAt = new Date().toISOString();
      if (!deletionEntryComplete(result, manifest.test50AuthProfile ?? 'real-os')) {
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
      process.stdout.write(result.status === 'business-passed-manual-pending'
        ? `🏁 Test #50 BUSINESS PASS — OS confirmation/Cancel MANUAL UNRUN; fullAcceptance=false\n`
        : `🏁 🟢 TEST SUCCESS — Test #${entry.testNumber}\n`);
      writeManifest(manifestPath, manifest);
      if (options.pauseAfterEachCase && !interruptedBy && options.diagnosticBlock === undefined
        && result.status === 'passed' && result.exitCode === 0 && result.signal === null && !result.error) {
        const next = entries.slice(entries.indexOf(entry) + 1).find(candidate => !manifest.tests.some(previous =>
          previous.test === candidate.testNumber && completedRangeEntry(previous, manifest.test50AuthProfile ?? 'real-os', runId)));
        if (next) {
          manifest.status = 'paused-maintenance';
          manifest.pendingTest = next.testNumber;
          manifest.failedTest = null;
          manifest.updatedAt = new Date().toISOString();
          collector.writeEvent(`maintenance pause after TEST #${entry.testNumber}; next whole test #${next.testNumber} has not started`);
          writeManifest(manifestPath, manifest);
          process.stdout.write(`⏸️ Maintenance pause after Test #${entry.testNumber}; resume the same run ID from Test #${next.testNumber} with the original --to.\n`);
          return 75;
        }
      }
    }

    if (options.diagnosticBlock !== undefined) {
      manifest.status = 'diagnostic-interrupted'; manifest.pendingTest = null; manifest.failedTest = null;
      manifest.finishedAt = new Date().toISOString(); writeManifest(manifestPath, manifest);
      return 130;
    }
    const manualPending = manifest.tests.some(result => result.status === 'business-passed-manual-pending');
    manifest.status = interruptedBy ? 'interrupted' : manualPending ? 'business-completed-manual-pending' : 'completed';
    if (manualPending) { manifest.fullAcceptance = false; manifest.manualOs = { status: 'unrun', confirmation: 'unrun', cancel: 'unrun' }; manifest.evidencePreserved = true; }
    manifest.finishedAt = new Date().toISOString();
    manifest.updatedAt = new Date().toISOString();
    manifest.pendingTest = null;
    manifest.failedTest = null;
    writeManifest(manifestPath, manifest);
    collector.writeEvent(`run ${runId} ${manifest.status}`);
    if (manifest.status === 'completed') {
      if (options.preserveEvidence || strictDeletion) {
        manifest.evidencePreserved = true;
        collector.writeEvent(strictDeletion ? 'Test50 evidence retained through owned-runtime cleanup' : 'successful-run artifact pruning skipped by --preserve-evidence');
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
  } catch (error) {
    primaryFailure = error;
    if (options.diagnosticBlock !== undefined) {
      manifest.status = 'diagnostic-failed'; manifest.pendingTest = null; manifest.failedTest = null;
      manifest.primaryFailure = collector.redactor.redactText(error.message);
      writeManifest(manifestPath, manifest);
    }
    if (strictDeletion) manifest.primaryFailure = collector.redactor.redactText(error.stack ?? error.message);
    throw error;
  } finally {
    await finishRangeOwnership({ manifest, manifestPath, collector, environmentGuard, strictDeletion, primaryFailure,
      retainHealthyOnPause: !interruptedBy && options.diagnosticBlock === undefined });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) main().then((code) => {
  process.exitCode = code;
}).catch((error) => {
  process.stderr.write(`E2E range runner failed before/around a test: ${error.stack ?? error.message}\n`);
  process.exitCode = 1;
});
