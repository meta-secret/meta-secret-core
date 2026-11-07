import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { runEntry, runPreparationCommand, assertAcceptedRunnerLineage } from '../run-ui-e2e-range.mjs';
import { prepareScenarioPlatforms, updateReadinessReport, resolveIosScenarioSimulator } from './platform-supervisor.mjs';
import { hashWorkingSources } from './batch-product-proof.mjs';
import { createBatchBuildHandoff } from './batch-build-handoff.mjs';
import { recordArtifactProvenance, verifyArtifactProvenance } from './build-provenance-manifest.mjs';
import { buildDeletionPlan, deletionCoverage, deletionEntryComplete } from '../Test50/plan.mjs';
import { E2E_BUILD_PROFILE, measurePreparationInputs, planPreparationBudget, createPreparationResourceBudget } from './e2e-resource-budget.mjs';
import { createPreparationMetrics } from './preparation-metrics.mjs';

test('optimization baseline: scenario error cannot finish the actor before its close barrier', async t => {
  const directory = realpathSync(mkdtempSync(resolve(tmpdir(), 'range-error-close-')));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const child = new EventEmitter(); child.stdout = new EventEmitter(); child.stderr = new EventEmitter();
  child.kill = () => assert.fail('Closed actor ports cannot signal a process');
  const failure = new Error('Controlled actor error before close'), finished = [], events = [], probeStates = [];
  let actorActive = false;
  const preparationMetrics = createPreparationMetrics({ now: () => 1n, observe: async () => {
    await Promise.resolve(); probeStates.push(actorActive); return { freeBytes: 64, swapUsedBytes: 0, swapTotalBytes: 0 }; } });
  let spawned; const started = new Promise(resolvePromise => { spawned = resolvePromise; });
  const path = resolve(directory, 'readiness.json'), log = resolve(directory, 'scenario.log'); writeFileSync(log, '');
  const pending = runEntry({ testNumber: 1, script: 'controlled-actor', scenarios: [{ value: scenario(1),
    path: new URL('../Test1/scenario.json', import.meta.url).pathname }] }, {
    attempt: 1, runId: 'error-close-fixture', preserveEvidence: true,
    collector: { runDirectory: directory, redactor: { redactText: text => text, addSensitiveValues() {} },
      startTest: () => log, writeEvent() {}, writeOutput() {}, finishTest: async value => { finished.push(value); } },
  }, { preparationMetrics, environmentGuard: { prepare: async () => {} }, batchProducts: { hasPersistedProof: true,
    nativePreparation: () => ({ android: true, ios: true, cli: false }), prepare: async () => '/controlled/products' } }, {
    prepareScenarioPlatforms: async () => { writeFileSync(path, JSON.stringify({ platforms: [], status: 'runtime-ready' }));
      return { platforms: [], reportPath: path }; }, resetE2eScenarioState: async () => ({}),
    startPlatformLivenessMonitor: () => () => events.push('monitor-stopped'), discoverAttemptArtifacts: () => [],
    spawn: () => { actorActive = true; queueMicrotask(spawned); return child; },
  });
  await Promise.race([started, pending.then(() => assert.fail('Controlled actor must be spawned'))]);
  child.emit('error', failure); await new Promise(setImmediate);
  try {
    assert.equal(finished.length, 0, 'An error notification is not actor close/drain authority');
    assert.deepEqual(events, [], 'Runtime monitoring must remain until actual actor close');
  } finally { actorActive = false; child.emit('close', 1, null); const result = await pending;
    assert.equal(result.status, 'failed'); assert.equal(result.error, failure.message); }
  assert.equal(finished.length, 1); assert.deepEqual(events, ['monitor-stopped']);
  assert(probeStates.length > 0); assert(probeStates.every(active => active === false), 'No resource probe may overlap the active actor');
});

test('optimization baseline: whole preparation refusal runs before the environment guard', async t => {
  const directory = realpathSync(mkdtempSync(resolve(tmpdir(), 'range-before-guard-')));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const calls = [], denied = new Error('Controlled whole-F2 resource refusal'), guardEntered = new Error('Guard entered without resource admission');
  const log = resolve(directory, 'scenario.log'); writeFileSync(log, '');
  const result = await runEntry({ testNumber: 1, script: 'controlled-preparation', scenarios: [{ value: scenario(1),
    path: new URL('../Test1/scenario.json', import.meta.url).pathname }] }, {
    attempt: 1, runId: 'before-guard-fixture', collector: { runDirectory: directory,
      redactor: { redactText: text => text, addSensitiveValues() {} }, startTest: () => log,
      writeEvent() {}, finishTest: async () => {} },
  }, { resourceBudget: { check: async () => { calls.push('budget'); throw denied; }, complete() {} },
    environmentGuard: { prepare: async () => { calls.push('guard'); throw guardEntered; } },
    batchProducts: { hasPersistedProof: true, nativePreparation: () => ({}), prepare: async () => assert.fail('No products after resource refusal') },
  }, { prepareScenarioPlatforms: async () => assert.fail('No SDK after resource refusal'),
    spawn: () => assert.fail('No child after resource refusal'), discoverAttemptArtifacts: () => [] });
  assert.deepEqual(calls, ['budget'], 'Whole resource admission must precede snapshot/selector/native work');
  assert.equal(result.status, 'readiness-failed'); assert.equal(result.error, denied.message);
});

test('optimization baseline: drained pending-test explicitly releases healthy ownership instead of stopping', async () => {
  const source = readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
  const first = source.indexOf('async function finishRangeOwnership('), last = source.indexOf('\n}\n', first) + 2;
  assert(first >= 0 && last > first);
  const runtime = { platform: 'Android', avdName: 'Pixel_4a', serial: 'controlled-runtime' }, events = [];
  const release = async (values, options) => { assert.equal(values[0], runtime); assert.equal(options.reason, 'pending-test');
    events.push('release'); return [{ platform: 'Android', reason: 'pending-test', disposition: 'released-healthy' }]; };
  const stop = async values => { assert.equal(values[0], runtime); events.push('stop'); };
  const finish = runInNewContext(source.slice(first, last) + '\nfinishRangeOwnership;', {
    stopRunnerOwnedRuntimes: stop, releaseHealthyRunnerOwnedRuntimes: release,
    writeManifest: () => events.push('manifest'), process: { stdout: { write() {} }, stderr: { write() {} } },
    Date, AggregateError,
  });
  const manifest = { status: 'failed', pendingTest: 5, tests: [{ test: 5, status: 'readiness-failed' }], runtimeOwnership: [runtime] };
  await finish({ manifest, manifestPath: '/controlled/run.json', strictDeletion: false,
    retainHealthyOnPause: true, services: { releaseHealthyRunnerOwnedRuntimes: release, stopRunnerOwnedRuntimes: stop },
    environmentGuard: { restore: async () => events.push('restore') },
    collector: { writeEvent() {}, close: async () => events.push('collector-close') } });
  assert.equal(events[0], 'release', 'A drained pending technical pause must not send a stop command');
  assert.equal(events.includes('stop'), false); assert.equal(manifest.status, 'failed');
  assert.deepEqual(Array.from(manifest.runtimeOwnership), []); assert(events.indexOf('restore') > events.indexOf('release'));
});

test('actual range finish retains old stop policy for business failure, unknown primary, completed and signalled disposition', async () => {
  const source = readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
  const first = source.indexOf('async function finishRangeOwnership('), last = source.indexOf('\n}\n', first) + 2;
  for (const kind of ['business-failed', 'unknown-primary', 'completed', 'signal']) {
    const runtime = { platform: 'Android' }, events = [], primary = Error('controlled unknown primary');
    const stop = async values => { assert.equal(values[0], runtime); events.push('stop'); };
    const release = async () => assert.fail('Only an explicitly retained, drained technical pause can release healthy ownership');
    const finish = runInNewContext(source.slice(first, last) + '\nfinishRangeOwnership;', {
      stopRunnerOwnedRuntimes: stop, releaseHealthyRunnerOwnedRuntimes: release, writeManifest() {},
      process: { stdout: { write() {} }, stderr: { write() {} } }, Date, AggregateError,
    });
    const manifest = { status: kind === 'completed' ? 'passed' : 'failed', pendingTest: 5, runtimeOwnership: [runtime],
      tests: [{ test: 5, status: kind === 'business-failed' ? 'failed' : 'readiness-failed' }] };
    await finish({ manifest, manifestPath: '/controlled/run.json', strictDeletion: false,
      primaryFailure: kind === 'unknown-primary' ? primary : undefined, retainHealthyOnPause: kind !== 'signal',
      services: { stopRunnerOwnedRuntimes: stop, releaseHealthyRunnerOwnedRuntimes: release },
      environmentGuard: { restore: async () => events.push('restore') }, collector: { writeEvent() {}, close: async () => events.push('close') } });
    assert.deepEqual(events, ['stop', 'restore', 'close']); assert.equal(manifest.runtimeReleases, undefined);
    assert.equal(manifest.status, kind === 'completed' ? 'passed' : 'failed');
  }
});

test('actual range measures the full selected inventory and rejects resources before guard preparation', async t => {
  const directory = realpathSync(mkdtempSync(resolve(tmpdir(), 'range-whole-budget-')));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const source = readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
  const first = source.indexOf('async function finishRangeOwnership('), start = source.indexOf('async function executeRange(');
  const last = source.indexOf('\n}\n', start) + 2, events = [], log = resolve(directory, 'scenario.log'); writeFileSync(log, '');
  const manifest = { runId: 'whole-budget-fixture', range: { from: 1, to: 1 }, status: 'running', tests: [] };
  let ledger, metrics;
  const enter = runInNewContext(source.slice(first, last) + '\nexecuteRange;', {
    coreRoot: resolve(directory, 'core'), composeRoot: resolve(directory, 'compose'), resolve, E2E_BUILD_PROFILE,
    assertExistingDeviceRun() {}, createScenarioRuntimeProfile: ({ testNumber }) => ({ kind: 'ui', testNumber }),
    measurePreparationInputs: options => { events.push('measure'); return measurePreparationInputs({ ...options, measureTreeBytes: () => 1 }); },
    planPreparationBudget: input => { events.push('plan'); return planPreparationBudget(input); },
    readPreparationObservation: async () => { events.push('observe'); return { freeBytes: 0,
      physicalBytes: 16 * 1024 ** 3, availableBytes: 3 * 1024 ** 3, swapUsedBytes: 0, swapTotalBytes: 0 }; },
    createPreparationResourceBudget: options => (ledger = createPreparationResourceBudget(options)),
    createPreparationMetrics: options => (metrics = createPreparationMetrics(options)),
    createRunnerAudit: async options => { assert.equal(options.runId, manifest.runId); events.push('closed-f1-audit'); return { path: '/controlled/audit', sha256: 'controlled' }; },
    validateRunnerAudit: () => ({}), assertAcceptedRunnerLineage,
    batchEnvironmentPaths: () => ({}), createBatchEnvironmentGuard: () => ({
      prepare: async () => events.push('guard-prepare'), restore: async () => events.push('restore') }),
    createBatchProductPreparation: options => { assert.equal(options.resourceBudget, ledger); assert.equal(options.preparationMetrics, metrics);
      return { hasPersistedProof: true, nativePreparation: () => ({ android: true, ios: true }),
        prepare: async () => assert.fail('Unsafe resources cannot reach products') }; },
    runPreparationCommand: () => assert.fail('No external preparation process'),
    runEntry: (entry, run, preparation) => runEntry(entry, run, preparation, {
      prepareScenarioPlatforms: async () => assert.fail('No SDK after budget refusal'), spawn: () => assert.fail('No actor after refusal'),
      discoverAttemptArtifacts: () => [] }),
    readFreeDiskGiB: () => 64, announce() {}, reportTestStage() {}, writeManifest() {}, writeFileSync,
    deletionEntryComplete, interruptedBy: null, diskThresholdGiB: 15, postCleanupMinimumGiB: 12,
    stopRunnerOwnedRuntimes: async () => assert.fail('No runtime was started'),
    releaseHealthyRunnerOwnedRuntimes: async () => assert.fail('No runtime was started'),
    process: { stdout: { write() {} }, stderr: { write() {} } },
  });
  const code = await enter({ entries: [{ testNumber: 1, script: 'controlled-only', scenarios: [{ value: scenario(1),
    path: new URL('../Test1/scenario.json', import.meta.url).pathname }] }], options: { preserveEvidence: true },
    runId: manifest.runId, runDirectory: directory, manifestPath: resolve(directory, 'run.json'), manifest,
    collector: { runDirectory: directory, redactor: { redactText: text => text, addSensitiveValues() {} },
      startTest: () => log, writeEvent() {}, writeOutput() {}, finishTest: async () => {}, close: async () => events.push('closed') } });
  assert.equal(code, 1); assert.equal(events[0], 'measure'); assert(events.indexOf('plan') < events.indexOf('observe'));
  assert.equal(events.includes('guard-prepare'), false); assert.equal(manifest.pendingTest, 1);
  assert.equal(manifest.tests[0].status, 'readiness-failed'); assert.equal(manifest.preparationBudget.androidConfigurationCount, 1);
  assert.equal(manifest.preparationProfile.gradleHeapBytes, 3 * 1024 ** 3); assert.equal(manifest.preparationProfile.cargoJobs, 2);
  assert(events.includes('restore')); assert.equal(events.at(-1), 'closed');
});

const scenario = (number) => JSON.parse(readFileSync(new URL(`../Test${number}/scenario.json`, import.meta.url), 'utf8'));
function childFixture(onSpawn) {
  const child = new EventEmitter(); child.stdout = new EventEmitter(); child.stderr = new EventEmitter();
  child.kill = () => { throw new Error('No actual platform cancellation is expected in this caller fixture'); };
  queueMicrotask(() => onSpawn(child)); return child;
}
async function runCallerFixture(testNumber, publishProof, profile, preparationMetrics) {
  const directory = realpathSync(mkdtempSync(resolve(tmpdir(), 'f2-coordinator-caller-'))); const calls = []; const value = scenario(testNumber);
  let selected; let childEnvironment; const finished = []; const iosUdid = profile?.udid ?? 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30';
  const logPath = resolve(directory, 'scenario.log'); writeFileSync(logPath, 'controlled caller fixture\n');
  const collector = { runDirectory: directory, redactor: { redactText: (text) => text, addSensitiveValues: () => {} }, startTest: () => logPath,
    writeEvent: () => {}, writeOutput: () => {}, finishTest: async (result) => { finished.push(result); } };
  const publish = (path) => {
    const previous = process.env.E2E_READINESS_PATH; process.env.E2E_READINESS_PATH = path;
    try {
      if (testNumber === 20) {
        updateReadinessReport({ platform: 'CLI/Core + local HTTP server', checks: { cargo: 'fixture cargo', sqlite3: 'fixture sqlite', coreManifest: '/fixture/Cargo.toml', serverMigration: '/fixture/up.sql' } });
        updateReadinessReport({ platform: 'isolated local HTTP server', checks: { url: 'http://127.0.0.1:3000', healthEndpoint: '/hi', healthy: true } });
      } else {
        updateReadinessReport({ platform: 'Android', checks: { serial: 'fixture-serial', appInstalled: true, instrumentationInstalled: true } });
        updateReadinessReport({ platform: 'iOS', checks: { simulatorUdid: iosUdid, appInstalled: true, testRunnerInstalled: true } });
      }
    } finally { if (previous === undefined) delete process.env.E2E_READINESS_PATH; else process.env.E2E_READINESS_PATH = previous; }
  };
  try {
    const result = await runEntry({ testNumber, script: `test-${testNumber}`, scenarios: [{ value, path: new URL(`../Test${testNumber}/scenario.json`, import.meta.url).pathname }] },
      { attempt: 1, collector, runId: 'caller-fixture', onRuntimeStarted: () => assert.fail('No real runtime may start'), preserveEvidence: true },
      { preparationMetrics, getIosProfile: profile ? async (name) => { calls.push('ios-profile'); assert.equal(name, 'iPhone 17'); return profile; } : undefined, environmentGuard: { prepare: async () => { calls.push('environment'); } },
        batchProducts: { hasPersistedProof: true, prepare: async () => { calls.push('products'); return '/fixture/batch-products.json'; },
          nativePreparation: () => ({ android: testNumber !== 20, cli: false }) } },
      { prepareScenarioPlatforms: async (options) => {
          calls.push('platforms'); selected = options.runtimeProfile;
          assert.equal(options.scenario, value); assert.equal(selected.testNumber, testNumber); assert.deepEqual(options.ownedIosProfile, profile);
          if (testNumber === 20) return prepareScenarioPlatforms({ ...options, verifyEnvironment: () => ({ mode: 'local', selectors: [] }) });
          const platforms = [{ platform: 'Android', avdName: 'Pixel_4a', serial: 'fixture-serial' }, { platform: 'iOS', simulatorName: 'iPhone 17', udid: iosUdid, ...(profile ? { ownedIosProfile: profile } : {}) }];
          const reportPath = resolve(directory, 'readiness.json'); writeFileSync(reportPath, JSON.stringify({ runId: 'caller-fixture', platforms, runtimeProfile: selected, status: 'runtime-ready' }));
          return { platforms, reportPath };
        },
        resetE2eScenarioState: async (options) => { calls.push('reset'); assert.equal(testNumber, 1); assert.equal(options.scenario, value); if (profile) { assert.equal(options.runId, profile.runId); assert.equal(options.readinessPath, resolve(directory, 'readiness.json')); assert.deepEqual(options.platforms[1].ownedIosProfile, profile); } return { serverReset: true, platformsReset: ['Android', 'iOS'] }; },
        startPlatformLivenessMonitor: (options) => { calls.push('monitor'); assert.equal(options.testNumber, testNumber); return () => { calls.push('monitor-stopped'); }; },
        discoverAttemptArtifacts: () => [],
        spawn: (file, args, options) => {
          calls.push('child'); assert.equal(file, 'npm'); assert.deepEqual(args, ['run', `test-${testNumber}`]); childEnvironment = options.env;
          const initial = JSON.parse(readFileSync(options.env.E2E_READINESS_PATH, 'utf8'));
          assert.notEqual(initial.status, 'ready'); assert.equal(initial.runtimeProfile.kind, selected.kind);
          if (profile) assert.equal(resolveIosScenarioSimulator({ simulatorName: 'iPhone 17', inventory: { devices: { [profile.runtimeIdentifier]: [{ name: profile.name, udid: profile.udid, state: 'Booted', deviceTypeIdentifier: profile.deviceTypeIdentifier }] } }, readinessPath: options.env.E2E_READINESS_PATH, runId: profile.runId }).udid, profile.udid);
          if (testNumber === 20) assert.deepEqual(initial.requiredNonPlatformParticipants, ['CLI/Core + local HTTP server', 'isolated local HTTP server']);
          return childFixture((child) => { if (publishProof) publish(options.env.E2E_READINESS_PATH); child.emit('close', 0, null); });
        },
      });
    assert.deepEqual(calls, testNumber === 20 ? ['environment', 'products', 'platforms', 'child', 'monitor', 'monitor-stopped']
      : ['environment', 'products', ...(profile ? ['ios-profile'] : []), 'platforms', 'reset', 'child', 'monitor', 'monitor-stopped']);
    assert.equal(selected.kind, testNumber === 20 ? 'core-http' : 'ui'); assert.equal(childEnvironment.E2E_TEST_NUMBER, String(testNumber));
    assert.equal(childEnvironment.E2E_RANGE_RUN, '1'); assert.equal(childEnvironment.E2E_PRESERVE_EVIDENCE, '1');
    if (testNumber === 20) {
      assert.equal(childEnvironment.E2E_BATCH_PRODUCTS_PATH, undefined); assert.equal(childEnvironment.E2E_REUSE_PROVEN_ARTIFACTS, undefined);
      const report = JSON.parse(readFileSync(result.readinessPath, 'utf8')); assert.equal(report.stateReset.ownership, 'child-owned Test20 isolated SQLite + actual meta-server');
    } else { assert.equal(childEnvironment.E2E_BATCH_PRODUCTS_PATH, '/fixture/batch-products.json'); assert.equal(childEnvironment.E2E_REUSE_PROVEN_ARTIFACTS, '1'); }
    assert.equal(result.status, publishProof ? 'passed' : 'readiness-failed'); assert.equal(result.exitCode, 0); assert.equal(finished.length, 1);
    if (!publishProof) assert.match(result.error, /readiness|proof/i);
  } finally { rmSync(directory, { recursive: true, force: true }); }
}
for (const testNumber of [1, 20]) for (const publishProof of [true, false]) {
  test(`F2 real coordinator: Test${testNumber} binds its profile and ${publishProof ? 'accepts complete' : 'rejects missing'} child proof`, { timeout: 5000 }, () => runCallerFixture(testNumber, publishProof));
}

test('asynchronous metric observations cannot lose a fast actor close before readiness validation', { timeout: 1000 }, async () => {
  let clock = 0n;
  const metrics = createPreparationMetrics({ now: () => ++clock, observe: async () => {
    await Promise.resolve(); return { freeBytes: 64, swapUsedBytes: 0, swapTotalBytes: 0 }; } });
  await runCallerFixture(1, true, undefined, metrics);
  const records = metrics.snapshot(); assert(records.some(record => record.phase === 'business-scenario'));
  assert(records.every(record => record.outcome === 0 && record.measurementErrors === 0));
});

function inventoryFixture() {
  const directory = mkdtempSync(resolve(tmpdir(), 'f2-full-inventory-')); const names = [];
  for (let index = 0; index < 1100; index++) {
    const name = `source-${String(index).padStart(4, '0')}-${'bounded-name-'.repeat(5)}.rs`;
    names.push(name); writeFileSync(resolve(directory, name), `fixture ${index}`);
  }
  const inventory = `${names.join('\0')}\0`; assert(Buffer.byteLength(inventory) > 65536);
  const trace = []; let settled = false;
  const ports = {
    spawn: (file, args) => {
      assert.equal(file, 'git'); assert.equal(args[0], 'ls-files'); trace.push('spawn');
      return childFixture((child) => {
        child.stdout.emit('data', Buffer.from(inventory.slice(0, 50000)));
        child.stdout.emit('data', Buffer.from(inventory.slice(50000)));
        if (!child.cancelled) { settled = true; trace.push('close'); child.emit('close', 0, null); }
      });
    },
    signal: (child, signal) => {
      trace.push(signal); child.cancelled = true;
      queueMicrotask(() => { settled = true; trace.push('close'); child.emit('close', 143, signal); });
    },
  };
  return { directory, names, inventory, trace, ports, settled: () => settled, cleanup: () => rmSync(directory, { recursive: true, force: true }) };
}
test('F2 real capture port: source inventory larger than65536 is complete and agrees with the receipt authority', { timeout: 5000 }, async () => {
  const f = inventoryFixture(); let captured;
  try {
    const sourceFingerprint = await hashWorkingSources({ root: f.directory, scopes: ['fixture'], runAndCapture: async (file, args, options) => {
      captured = await runPreparationCommand(file, args, { ...options, ports: f.ports, timeoutMs: 2000 }); return captured;
    } });
    assert.equal(captured.stdout, f.inventory); assert(Buffer.byteLength(captured.diagnosticTails.stdout) <= 65536);
    const expected = createHash('sha256'); for (const name of f.names.sort()) expected.update(`${name}\0`).update(readFileSync(resolve(f.directory, name)));
    assert.equal(sourceFingerprint, expected.digest('hex')); assert.deepEqual(f.trace, ['spawn', 'close']); assert.equal(f.settled(), true);
    const path = resolve(f.directory, 'proof.json'); const current = { version: 3, sourceFingerprint, artifacts: { output: 'fixture' } };
    recordArtifactProvenance(path, current, 'controlled complete-inventory fixture'); assert.equal(verifyArtifactProvenance({ path, current }).sourceFingerprint, sourceFingerprint);
  } finally { f.cleanup(); }
});
test('F2 real capture port: explicit overflow cancels and drains before any builder or partial proof, even with allowFailure', { timeout: 5000 }, async () => {
  const f = inventoryFixture(); let builders = 0;
  try {
    const handoff = createBatchBuildHandoff({ runDirectory: resolve(f.directory, 'run'),
      captureInputs: async () => ({ sourceFingerprint: await hashWorkingSources({ root: f.directory, scopes: ['fixture'],
        runAndCapture: (file, args, options) => runPreparationCommand(file, args, { ...options, ports: f.ports, maxCaptureBytes: 65536, allowFailure: true, timeoutMs: 2000 }) }) }),
      builders: { wasm: async () => { builders++; throw new Error('A partial inventory cannot reach a builder'); } }, inspectProducts: () => assert.fail('No product inspection after overflow'),
    });
    await assert.rejects(handoff.prepare({ product: 'wasm', configuration: { environment: 'local' }, target: { platform: 'web' } }), (error) => {
      assert.equal(error.code, 'PREPARATION_CAPTURE_OVERFLOW'); assert(error.captureDiagnostics.stdout.bytes > 65536);
      assert(error.captureDiagnostics.stdout.tailBytes <= 65536); assert(!error.message.includes('source-')); return true;
    });
    assert.equal(builders, 0); assert.equal(f.settled(), true); assert.deepEqual(f.trace, ['spawn', 'SIGTERM', 'close']);
    assert.equal(existsSync(resolve(f.directory, 'run/products')), false); assert.equal(readdirSync(f.directory).some((name) => name.includes('provenance')), false);
  } finally { f.cleanup(); }
});


test('foreign created-profile caller is refused before reset or child spawn', async t => {
  const directory = realpathSync(mkdtempSync(resolve(tmpdir(), 'foreign-profile-refusal-')));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const logPath = resolve(directory, 'log'); writeFileSync(logPath, ''); const calls = [];
  const result = await runEntry({ testNumber: 1, script: 'controlled-only', scenarios: [{
    value: scenario(1), path: new URL('../Test1/scenario.json', import.meta.url).pathname,
  }] }, { attempt: 1, runId: 'refusal-fixture', collector: { runDirectory: directory,
    redactor: { redactText: text => text, addSensitiveValues: () => {} },
    startTest: () => logPath, writeEvent: () => {}, finishTest: async () => {},
  } }, { environmentGuard: { prepare: async () => {} }, batchProducts: {
    hasPersistedProof: true, prepare: async () => '/controlled/products', nativePreparation: () => ({}),
  } }, { prepareScenarioPlatforms: async () => ({ platforms: [{ platform: 'iOS',
    udid: '11111111-1111-4111-8111-111111111111', ownedIosProfile: { createdByRun: true },
  }] }), resetE2eScenarioState: async () => calls.push('reset'),
    spawn: () => calls.push('child'), discoverAttemptArtifacts: () => [] });
  assert.equal(result.status, 'readiness-failed'); assert.match(result.error, /owned|existing|profile|reuse/i);
  assert.deepEqual(calls, []);
});

test('actual Test50 caller refuses every host-known conflict before identity or any mutation port', async t => {
  const directory = realpathSync(mkdtempSync(resolve(tmpdir(), 'profile-refusal-')));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const keys = ['E2E_TEST50_AUTH_PROFILE', 'METASECRET_UI_TEST_MODE', 'SIMCTL_CHILD_METASECRET_UI_TEST_MODE',
    'E2E_ANDROID_DEVICE_PASSCODE', 'E2E_IOS_DEVICE_PASSCODE', 'JAVA_TOOL_OPTIONS'];
  const saved = Object.fromEntries(keys.map(key => [key, process.env[key]]));
  const conflicts = [ { E2E_TEST50_AUTH_PROFILE: '' },
    ...keys.slice(1, 5).flatMap(key => ['', 'false'].map(value => ({ [key]: value }))),
    { JAVA_TOOL_OPTIONS: '-Dmetasecret.test.force_system_auth=true' } ];
  try {
    for (const conflict of conflicts) {
      for (const key of keys) delete process.env[key];
      Object.assign(process.env, { E2E_TEST50_AUTH_PROFILE: 'controlled-business', ...conflict });
      const calls = [], port = async () => { calls.push('mutation'); throw Error('Unexpected mutation'); };
      await assert.rejects(runEntry({ testNumber: 50, script: 'controlled-only', scenarios: [{ value: scenario(50), path: resolve(directory, 'scenario.json') }] },
        { runId: 'profile-refusal', attempt: 1, collector: { runDirectory: directory, startTest: () => calls.push('log') } },
        { environmentGuard: { prepare: port }, batchProducts: { prepare: port } },
        { prepareScenarioPlatforms: port, resetE2eScenarioState: port, spawn: port }));
      assert.deepEqual(calls, []);
      assert.equal(existsSync(resolve(directory, 'ios-identities')), false);
    }
  } finally {
    for (const key of keys) if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key];
  }
});

async function runProfileCallerFixture(t, variant) {
  const directory = realpathSync(mkdtempSync(resolve(tmpdir(), 'profile-caller-')));
  const runId = 'profile-port-' + createHash('sha256').update(directory).digest('hex').slice(0, 16);
  const configDirectory = resolve(new URL('..', import.meta.url).pathname, '.artifacts', runId, 'test50-config');
  assert.equal(existsSync(resolve(configDirectory, '..')), false);
  mkdirSync(configDirectory, { recursive: true });
  t.after(() => { rmSync(directory, { recursive: true, force: true }); rmSync(resolve(configDirectory, '..'), { recursive: true, force: true }); });
  const authProfile = 'controlled-business', value = scenario(50), attempt = 1;
  const blocks = buildDeletionPlan(runId).blocks.map(block => {
    const raw = JSON.stringify({ authProfile, deletionBlock: block });
    writeFileSync(resolve(configDirectory, `scenario-${block.id}.json`), raw);
    return { id: block.id, cycles: block.cycles.map(cycle => cycle.number), configurationSha256: createHash('sha256').update(raw).digest('hex') };
  });
  const receipt = { version: 1, runId, attempt, authProfile, stage: 'F4', blocks,
    business: { status: 'passed', blocks: 8, cycles: 60 }, outcome: { code: 0, signal: null, cleanupErrors: [] }, ...deletionCoverage(authProfile) };
  if (variant === 'partial') receipt.business.cycles--;
  if (variant === 'wrong-profile') receipt.authProfile = 'real-os';
  if (variant === 'cleanup-failure') receipt.outcome.cleanupErrors.push('controlled cleanup failure');
  if (variant !== 'missing') writeFileSync(resolve(configDirectory, 'result-attempt-1.json'), JSON.stringify(receipt));
  const logPath = resolve(directory, 'caller.log'); writeFileSync(logPath, 'controlled caller fixture');
  const reportPath = resolve(directory, 'readiness.json'), calls = [], finished = [];
  const platforms = [{ platform: 'Android', avdName: 'Pixel_4a', serial: 'fixture-existing-serial', installation: { verified: true } },
    { platform: 'iOS', simulatorName: 'iPhone 17', udid: 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30', installation: { verified: true } }];
  const collector = { runDirectory: directory, redactor: { redactText: text => text, addSensitiveValues: () => {} },
    startTest: () => logPath, writeEvent: () => {}, writeOutput: () => {}, finishTest: async result => finished.push(result) };
  const result = await runEntry({ testNumber: 50, script: 'test-50', scenarios: [{ value, path: new URL('../Test50/scenario.json', import.meta.url).pathname }] },
    { runId, attempt, collector, test50AuthProfile: authProfile },
    { environmentGuard: { prepare: async () => calls.push('environment') }, batchProducts: { hasPersistedProof: true,
      prepare: async () => '/fixture/products', nativePreparation: () => ({}) } }, {
      prepareScenarioPlatforms: async options => {
        calls.push('platforms'); writeFileSync(reportPath, JSON.stringify({ runId, status: 'runtime-ready', platforms, runtimeProfile: options.runtimeProfile }));
        return { platforms, reportPath };
      }, resetE2eScenarioState: async () => { calls.push('reset'); return { serverReset: true }; },
      startPlatformLivenessMonitor: () => () => calls.push('monitor-closed'), discoverAttemptArtifacts: () => [],
      spawn: (_file, _args, options) => {
        calls.push('child'); assert.equal(options.env.E2E_TEST50_AUTH_PROFILE, authProfile);
        assert.equal(options.env.E2E_PRESERVE_EVIDENCE, '1');
        return childFixture(child => {
          const report = JSON.parse(readFileSync(reportPath)); report.status = 'ready'; writeFileSync(reportPath, JSON.stringify(report));
          child.emit('close', variant === 'nonzero' ? 1 : 0, variant === 'signal' ? 'SIGTERM' : null);
        });
      },
    });
  assert.deepEqual(calls, ['environment', 'platforms', 'reset', 'child', 'monitor-closed']);
  assert.equal(finished.length, 1);
  assert.equal(result.status, variant === 'valid' ? 'business-passed-manual-pending' : 'failed');
  assert.equal(deletionEntryComplete(result, authProfile), variant === 'valid');
  assert.equal(deletionEntryComplete(result, 'real-os'), false);
  if (variant === 'valid') { assert.equal(result.acceptance.fullAcceptance, false); assert.equal(result.acceptance.manualOs.status, 'unrun'); }
}

test('actual Test50 caller accepts scoped8/60 receipt and rejects missing partial cross-profile failed or signalled proof', async t => {
  for (const variant of ['valid', 'missing', 'partial', 'wrong-profile', 'cleanup-failure', 'nonzero', 'signal']) await runProfileCallerFixture(t, variant);
});

// Actual runner function bodies, controlled external ports; no process/device/SDK starts.
function maintenanceBody(name) {
  const source = readFileSync(new URL('../run-ui-e2e-range.mjs', import.meta.url), 'utf8');
  const start = source.indexOf(`async function ${name}(`), end = source.indexOf('\n}\n', start) + 2;
  assert(start >= 0 && end > start, `Complete actual ${name} declaration required`);
  return source.slice(start, end);
}
async function maintenanceRange(t, settings = {}) {
  const directory = realpathSync(mkdtempSync(resolve(tmpdir(), 'range-maintenance-')));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const events = [], snapshots = [], children = [], actorResults = [], udid = 'DB22263E-A8A3-49C5-ADD1-BAE4089E2E30';
  const numbers = settings.numbers ?? [1, 2];
  const entries = numbers.map(testNumber => ({ testNumber, script: `test-${testNumber}`,
    scenarios: [{ value: scenario(testNumber), path: new URL(`../Test${testNumber}/scenario.json`, import.meta.url).pathname }] }));
  const manifest = { runId: 'maintenance-owner', range: { from: numbers[0], to: numbers.at(-1) }, status: 'running',
    tests: settings.accepted ? settings.accepted.map(test => ({ test, status: 'passed', exitCode: 0, signal: null, error: null })) : [], runtimeOwnership: [] };
  const collector = { runDirectory: directory, redactor: { redactText: text => text, addSensitiveValues() {} },
    startTest: number => { const path = resolve(directory, `test-${number}.log`); writeFileSync(path, ''); return path; },
    writeEvent: message => events.push(message), writeOutput() {}, finishTest: async value => events.push(`finish:${value.status}`),
    close: async () => { events.push('collector-close'); if (settings.failClose) throw Error('controlled collector failure'); } };
  const guard = { prepare: async () => events.push('prepare'), restore: async () => { events.push('restore'); if (settings.failRestore) throw Error('controlled restore failure'); } };
  const prepared = { hasPersistedProof: true, nativePreparation: () => ({ android: true, ios: true, cli: false }), prepare: async () => '/controlled/batch-products.json' };
  let entered; const spawned = new Promise(accept => { entered = accept; });
  const ports = {
    coreRoot: resolve(directory, 'core'), composeRoot: resolve(directory, 'compose'), resolve, Date, AggregateError, E2E_BUILD_PROFILE,
    assertExistingDeviceRun() {}, createScenarioRuntimeProfile: ({ testNumber }) => ({ kind: 'ui', testNumber }),
    measurePreparationInputs: () => ({}), planPreparationBudget: () => ({}), createPreparationResourceBudget: () => ({ check: async () => {}, complete() {} }),
    readPreparationObservation: async () => ({}), createPreparationMetrics: () => ({ measure: async (_name, operation) => operation() }),
    createRunnerAudit: async () => ({ path: '/controlled/audit' }), validateRunnerAudit: () => ({}), assertAcceptedRunnerLineage() {},
    batchEnvironmentPaths: () => ({}), createBatchEnvironmentGuard: () => guard, createBatchProductPreparation: () => prepared,
    runPreparationCommand: () => assert.fail('No preparation command may escape'), readFreeDiskGiB: () => 64,
    announce() {}, reportTestStage() {}, writeFileSync,
    writeManifest: (_path, value) => { events.push(`journal:${value.status}`); snapshots.push(JSON.parse(JSON.stringify(value)));
      if (settings.failJournal && value.status === 'paused-maintenance') throw Error('controlled journal failure'); },
    deletionEntryComplete, completedRangeEntry: result => result.status === 'passed' && result.exitCode === 0 && result.signal === null && !result.error,
    interruptedBy: null, diskThresholdGiB: 15, postCleanupMinimumGiB: 12,
    stopRunnerOwnedRuntimes: async () => events.push('stop'), releaseHealthyRunnerOwnedRuntimes: async () => { events.push('release'); return []; },
    process: { stdout: { write() {} }, stderr: { write() {} } },
    runEntry: async (entry, run, preparation) => {
      if (settings.readinessFailure) {
        // Actual runEntry resource refusal, not a fabricated business result.
        preparation = { ...preparation, resourceBudget: { check: async () => { throw Error('controlled readiness refusal'); } } };
      }
      const result = await runEntry(entry, run, preparation, {
        prepareScenarioPlatforms: async () => {
          const reportPath = resolve(directory, `readiness-${entry.testNumber}.json`);
          const platforms = [{ platform: 'Android', avdName: 'Pixel_4a', serial: 'fixture-serial' },
            { platform: 'iOS', simulatorName: 'iPhone 17', udid }];
          writeFileSync(reportPath, JSON.stringify({ runId: manifest.runId, platforms, runtimeProfile: { kind: 'ui', testNumber: entry.testNumber }, status: 'runtime-ready' }));
          return { platforms, reportPath };
        }, resetE2eScenarioState: async () => ({}), startPlatformLivenessMonitor: () => () => events.push('monitor-stopped'), discoverAttemptArtifacts: () => [],
        spawn: (_file, _args, options) => {
          const child = new EventEmitter(); child.stdout = new EventEmitter(); child.stderr = new EventEmitter(); child.kill = () => assert.fail('No device signals');
          children.push(child); events.push(`spawn:${entry.testNumber}`);
          const publish = () => {
            const previous = process.env.E2E_READINESS_PATH; process.env.E2E_READINESS_PATH = options.env.E2E_READINESS_PATH;
            try {
              updateReadinessReport({ platform: 'Android', checks: { serial: 'fixture-serial', appInstalled: true, instrumentationInstalled: true } });
              updateReadinessReport({ platform: 'iOS', checks: { simulatorUdid: udid, appInstalled: true, testRunnerInstalled: true } });
            } finally { if (previous === undefined) delete process.env.E2E_READINESS_PATH; else process.env.E2E_READINESS_PATH = previous; }
          };
          queueMicrotask(() => { publish(); entered(); if (!settings.holdChild) {
            if (settings.actorError) child.emit('error', Error('controlled actor error'));
            child.emit('close', settings.exitCode ?? 0, settings.signal ?? null);
          } }); return child;
        },
      });
      actorResults.push(result);
      if (settings.interrupt) { result.status = 'interrupted'; ports.interruptedBy = 'SIGINT'; }
      return result;
    },
  };
  // Same context object is contextified by VM, so a controlled interruption is visible.
  const execute = runInNewContext(maintenanceBody('finishRangeOwnership') + '\n' + maintenanceBody('executeRange') + '\nexecuteRange;', ports);
  const pending = execute({ entries, options: { preserveEvidence: true, pauseAfterEachCase: settings.enabled !== false },
    runId: manifest.runId, runDirectory: directory, manifestPath: resolve(directory, 'run.json'), manifest, collector });
  return { pending, spawned, events, snapshots, children, actorResults, manifest };
}

test('maintenance actual loop journals one closed whole PASS, preserves next UNRUN, then restores and closes', { timeout: 5000 }, async t => {
  const f = await maintenanceRange(t); assert.equal(await f.pending, 75);
  assert.equal(f.manifest.status, 'paused-maintenance'); assert.equal(f.manifest.pendingTest, 2); assert.equal(f.manifest.failedTest, null);
  assert.equal(f.manifest.tests.length, 1); assert.equal(f.manifest.tests[0], f.actorResults[0]); assert.equal(f.actorResults[0].status, 'passed');
  assert.equal(f.children.length, 1); assert(f.events.indexOf('finish:passed') < f.events.indexOf('journal:paused-maintenance'));
  assert(f.events.indexOf('journal:paused-maintenance') < f.events.indexOf('restore')); assert.equal(f.events.at(-1), 'collector-close');
  assert(f.snapshots.some(snapshot => snapshot.status === 'running' && snapshot.tests.length === 1), 'Whole PASS is journaled before checkpoint');
});

test('maintenance actual loop cannot checkpoint on actor error or exit before close', { timeout: 5000 }, async t => {
  const f = await maintenanceRange(t, { holdChild: true }); await f.spawned;
  f.children[0].emit('error', Error('controlled preclose error')); f.children[0].emit('exit', 0, null);
  await new Promise(setImmediate); assert.equal(f.manifest.tests.length, 0); assert(!f.events.includes('restore'));
  f.children[0].emit('close', 0, null); assert.equal(await f.pending, 1);
  assert.equal(f.manifest.status, 'failed'); assert.equal(f.manifest.pendingTest, 1); assert.equal(f.children.length, 1);
});

for (const settings of [{ enabled: false }, { numbers: [1] }]) {
  test(`maintenance unchanged completion ${JSON.stringify(settings)}`, { timeout: 5000 }, async t => {
    const f = await maintenanceRange(t, settings); assert.equal(await f.pending, 0); assert.equal(f.manifest.status, 'completed');
    assert.equal(f.manifest.pendingTest, null); assert.equal(f.manifest.tests.length, settings.numbers?.length ?? 2);
  });
}
for (const settings of [{ exitCode: 3 }, { signal: 'SIGTERM' }, { actorError: true }, { readinessFailure: true }, { interrupt: true }]) {
  test(`maintenance does not advance unsuccessful whole case ${JSON.stringify(settings)}`, { timeout: 5000 }, async t => {
    const f = await maintenanceRange(t, settings); assert.notEqual(await f.pending, 75);
    assert.equal(f.manifest.pendingTest, 1); assert.notEqual(f.manifest.status, 'paused-maintenance');
    assert.equal(f.manifest.tests.length, 1); assert.equal(f.children.length, settings.readinessFailure ? 0 : 1);
  });
}

test('maintenance next target follows canonical entries and skips already accepted entries', { timeout: 5000 }, async t => {
  const f = await maintenanceRange(t, { numbers: [17, 19, 22], accepted: [19] }); assert.equal(await f.pending, 75);
  assert.equal(f.manifest.pendingTest, 22); assert.deepEqual(f.manifest.tests.map(result => result.test), [19, 17]); assert.equal(f.children.length, 1);
});
for (const settings of [{ failJournal: true }, { failRestore: true }, { failClose: true }]) {
  test(`maintenance checkpoint failure is not successful return ${JSON.stringify(settings)}`, { timeout: 5000 }, async t => {
    const f = await maintenanceRange(t, settings); await assert.rejects(f.pending, /controlled|Range execution/);
    assert.equal(f.children.length, 1); assert.equal(f.manifest.tests.length, 1);
    if (settings.failRestore) assert.equal(f.manifest.status, 'restore-failed');
  });
}

test('maintenance exact healthy release uses existing finally policy and error fallback', async () => {
  for (const kind of ['healthy', 'release-failed', 'primary', 'signal', 'missing-pending', 'restore-failed', 'stop-failed']) {
    const events = [], runtime = { platform: 'Android' };
    const finish = runInNewContext(maintenanceBody('finishRangeOwnership') + '\nfinishRangeOwnership;', {
      stopRunnerOwnedRuntimes: async () => { events.push('stop'); if (kind === 'stop-failed') throw Error('controlled stop failure'); },
      releaseHealthyRunnerOwnedRuntimes: async () => { events.push('release'); if (['release-failed', 'stop-failed'].includes(kind)) throw Error('controlled release failure'); return [{ disposition: 'released-healthy' }]; },
      writeManifest() {}, process: { stdout: { write() {} }, stderr: { write() {} } }, Date, AggregateError,
    });
    const manifest = { status: 'paused-maintenance', pendingTest: kind === 'missing-pending' ? null : 2,
      tests: [{ test: 1, status: 'passed' }], runtimeOwnership: [runtime] };
    const work = finish({ manifest, manifestPath: '/controlled/run.json', strictDeletion: kind === 'stop-failed',
      primaryFailure: kind === 'primary' ? Error('controlled primary') : undefined, retainHealthyOnPause: kind !== 'signal',
      environmentGuard: { restore: async () => { events.push('restore'); if (kind === 'restore-failed') throw Error('controlled restore failure'); } },
      collector: { writeEvent() {}, close: async () => events.push('close') } });
    if (['restore-failed', 'stop-failed'].includes(kind)) await assert.rejects(work, /controlled|Range execution/); else await work;
    assert.equal(events.at(-1), 'close'); assert(events.includes('restore'));
    if (['primary', 'signal', 'missing-pending'].includes(kind)) assert.equal(events[0], 'stop'); else assert.equal(events[0], 'release');
    if (kind === 'release-failed') assert.deepEqual(events, ['release', 'stop', 'restore', 'close']);
  }
});

test('maintenance existing main resume requires exact pending, endpoint, identity and normal incomplete scope', async t => {
  const directory = realpathSync(mkdtempSync(resolve(tmpdir(), 'maintenance-resume-'))); t.after(() => rmSync(directory, { recursive: true, force: true }));
  const value = scenario(1), entries = [{ testNumber: 2, scenarios: [{ value }], script: 'test-2' }];
  for (const kind of ['valid', 'wrong-from', 'changed-to', 'wrong-id', 'completed', 'diagnostic']) {
    const stored = { runId: 'maintenance-owner', range: { from: 1, to: 49 }, status: 'paused-maintenance', pendingTest: 2, tests: [{ test: 1, status: 'passed' }] };
    if (kind === 'completed') stored.status = 'completed'; if (kind === 'wrong-id') stored.runId = 'foreign-owner';
    if (kind === 'diagnostic') stored.scope = { kind: 'diagnostic-block' };
    const owner = resolve(directory, 'maintenance-owner'); mkdirSync(owner, { recursive: true }); writeFileSync(resolve(owner, 'run.json'), JSON.stringify(stored));
    let forwarded = 0;
    const options = { from: kind === 'wrong-from' ? 3 : 2, to: kind === 'changed-to' ? 48 : 49, runId: 'maintenance-owner', pauseAfterEachCase: true };
    const main = runInNewContext(maintenanceBody('main') + '\nmain;', {
      parseOptions: () => options, process: { argv: [] }, resolveRange: () => ({ entries, skippedTests: [] }),
      testsRoot: directory, resolve, mkdirSync, existsSync, readFileSync, JSON,
      collectSensitiveValues() {}, RunArtifactCollector: class { writeEvent() {} },
      executeRange: async received => { forwarded++; assert.equal(received.manifest.tests[0].test, 1); assert.equal(received.manifest.status, 'running'); return 0; },
    });
    if (kind === 'valid') { assert.equal(await main(), 0); assert.equal(forwarded, 1); }
    else { await assert.rejects(main()); assert.equal(forwarded, 0); }
  }
});
