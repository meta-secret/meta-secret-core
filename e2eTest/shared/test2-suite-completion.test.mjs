import assert from 'node:assert/strict';
import test from 'node:test';
import { EventEmitter } from 'node:events';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import vm from 'node:vm';
import { createIosSelectedSuiteActor } from './ios-selected-suite.mjs';
import { createHarnessShutdown } from './harness-shutdown.mjs';
import { assertAndroidInstrumentationPassed } from './android-instrumentation.mjs';

const source = () => readFileSync(resolve(process.cwd(), 'Test2/orchestrator.mjs'), 'utf8');
const between = (first, last) => {
  const text = source(), start = text.indexOf(first), end = text.indexOf(last, start + first.length);
  assert(start >= 0 && end > start, 'Actual Test2 declaration boundary required');
  return text.slice(start, end);
};
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
};
const settle = async () => { for (let step = 0; step < 30; step++) await Promise.resolve(); };
const context = extra => vm.createContext({ console: { log() {}, warn() {}, error() {} },
  setTimeout, clearTimeout, Promise, Set, Map, resolve, ...extra });

async function clocked(t, work) {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const actors = [];
  const clock = { actors, async tick(ms) { t.mock.timers.tick(ms); await settle(); } };
  let primary, results;
  try { await work(clock); } catch (error) { primary = error; }
  finally {
    void Promise.allSettled(actors.map(actor => actor.cancelAndJoin())).then(value => { results = value; });
    for (let step = 0; step < 12 && !results; step++) { await settle(); t.mock.timers.tick(50); }
    await settle(); t.mock.timers.reset();
  }
  assert(results, 'Exact mock actor cleanup must finish');
  const failures = results.filter(row => row.status === 'rejected').map(row => row.reason);
  if (failures.length) throw new AggregateError(primary ? [primary, ...failures] : failures, 'Mock cleanup failed');
  if (primary) throw primary;
}

function actualWatcher() {
  const child = new EventEmitter(), signals = [];
  child.stdout = new EventEmitter(); child.stderr = new EventEmitter();
  Object.assign(child, { exitCode: null, signalCode: null, kill(signal) {
    signals.push(signal); finish(null, signal); return true;
  } });
  function finish(code, signal = null) {
    child.exitCode = code; child.signalCode = signal;
    child.emit('exit', code, signal); child.emit('close', code, signal);
  }
  const c = context({ run: () => child, watchedProcessOutputs: [] });
  vm.runInContext(between('function watchProcessOutput(', 'function normalizeAndroidRecoveryDiagnostic('), c);
  const watched = c.watchProcessOutput('controlled-native-wrapper', [], {});
  void watched.result.catch(() => {});
  return { child, watched, signals, finish,
    emit: line => child.stdout.emit('data', Buffer.from(line + '\n')) };
}

function declaredCycles() {
  const scenario = JSON.parse(readFileSync(resolve(process.cwd(), 'Test2/scenario.json'), 'utf8'));
  return scenario.recovery.groups.flatMap(group => Array.from({ length: group.count }, (_, index) => ({
    approver: group.approver.startsWith('alternate-') ? group.approver.split('-').slice(1)[index % 2] : group.approver,
  }))).map((cycle, index) => ({ ...cycle, number: index + 1 }));
}

function businessContext(extra = {}) {
  return context({ nativeBusinessFailure: undefined, nativeBusinessPromise: undefined,
    scenario: { secret: { name: 'controlled-item' }, vault: { name: 'controlled-vault' } },
    recoveryCycles: declaredCycles(), recoveryShowTimeoutMs: 45_000,
    androidRecoveryDiagnostics: { stateSequence: 0 }, approvalCoordinator: { allow() {} },
    openWebSecrets: async () => {}, openWebIncoming: async () => ({ requestId: 'controlled-request' }),
    decideWebIncoming: async () => {}, waitForAndroidPendingRecovery: async () => {},
    waitWebIncomingGone: async () => {}, unlockWithPasskeyIfNeeded: async () => {},
    requestWebVaultJoin: async () => {}, ...extra });
}

test('actual Test2 bridge uses the selected suite actor and joins its passed wrapper', async t => clocked(t, async clock => {
  const f = actualWatcher(), calls = [];
  const c = context({ batchProducts: {}, coreRoot: '/controlled/core', composeRoot: '/controlled/compose',
    iosDerivedDataPath: '/controlled/derived', iosProjectPath: '/controlled/project', iosResultBundlePath: '/controlled/result',
    iosIdentity: { context: { runRoot: '/controlled/run' } }, recoveryCycles: Array(18),
    iosRecoveryApprovalCycles: [], namespaceRedactor: {}, iosSuiteActors: [],
    scenario: { vault: { name: 'controlled-vault' }, secret: { name: 'controlled-item', value: 'controlled-value' }, ios: { bundleId: 'controlled.bundle' } },
    assertBatchProductsCurrent() {}, installBuiltIosTestApps: async () => {},
    runAndWait() { throw new Error('SDK execution is forbidden in this VM control'); },
    runAndCapture() { throw new Error('SDK execution is forbidden in this VM control'); },
    iosGeneratedTemplateProof: () => ({}), prepareScopedIosInvocation: () => ({ xctestrun: '/controlled/scoped' }),
    iosSerialTestArguments: () => ['controlled-existing-target'], watchProcessOutput: () => f.watched,
    createIosSelectedSuiteActor(options) {
      calls.push(options);
      return createIosSelectedSuiteActor({ ...options, naturalExitMs: 5, closeTimeoutMs: 5, killTimeoutMs: 2 });
    } });
  vm.runInContext(between('async function startIosJoinTest(', 'async function prepareAndroidEmulator('), c);
  const actor = await c.startIosJoinTest('controlled-existing-target');
  try {
    // On the old real bridge the raw watched object returns, then this exact assertion is 1 vs 0.
    assert.equal(calls.length, 1, 'Test2 must select its existing bounded iOS suite actor');
    clock.actors.push(actor);
    assert.equal(calls[0].selector, 'iosAppUITests/CaseTwoIosInitiatorUITest/testCreateVaultThenApproveAndroidAndWeb');
    assert.equal(calls[0].timeoutMs, 600_000); assert.equal(calls[0].deferCompletionTimeout, true);
    let state = 'pending'; void actor.result.then(() => { state = 'passed'; }, () => { state = 'failed'; });
    await clock.tick(600_001); assert.equal(state, 'pending');
    f.emit('E2E: IOS_RECOVERY_CLOSED_18'); f.emit("Test Suite 'Selected tests' passed");
    await settle(); assert.deepEqual(f.signals, []);
    await clock.tick(5); await actor.result;
    assert.equal(state, 'passed'); assert.deepEqual(f.signals, ['SIGTERM']);
    await actor.closed;
    // Cached real business markers still win over the now-cancelled future observations.
    await actor.waitForMarker('E2E: IOS_RECOVERY_CLOSED_18');
    await assert.rejects(actor.waitForMarker('NEVER_EMITTED'), /controlled-native-wrapper exited with SIGTERM/);
  } finally { if (f.child.exitCode === null && f.child.signalCode === null) f.finish(0); }
}));

test('actual Test2 watcher keeps Android result pending through exit and validates late bytes at close', async () => {
  const f = actualWatcher(); let settled = false;
  void f.watched.result.then(() => { settled = true; });
  f.emit('OK (1 test)'); f.child.exitCode = 0; f.child.emit('exit', 0, null);
  await settle(); assert.equal(settled, false, 'exit alone cannot complete the original Android result');
  f.emit('FAILURES!!!'); f.child.emit('close', 0, null);
  const output = await f.watched.result; await f.watched.closed;
  assert.throws(() => assertAndroidInstrumentationPassed(output, 'controlled.Selected#method'), /instrumentation failed/);
  assert.deepEqual(f.signals, []);
});

test('actual Test2 keeps all eighteen cycles WebGone Android natural result and iOS join before whole completion', async () => {
  const waits = [], allows = [], gone = [], finalWeb = deferred(), iosJoin = deferred(), androidResult = deferred();
  let completeCalls = 0, finished = false;
  const android = { result: androidResult.promise, waitForMarker: async marker => { waits.push(marker); } };
  const ios = { result: deferred().promise, assertBusinessActive() {}, waitForMarker: async (marker, timeout) => { waits.push(marker); if (marker.includes('SECRET_VISIBLE_')) assert.equal(timeout, 45_000); },
    complete() { completeCalls++; return iosJoin.promise; } };
  const c = businessContext({ approvalCoordinator: { allow(kind, number) { allows.push([kind, number]); } },
    startAndroidJoinTest: async () => android,
    waitWebIncomingGone: async () => { gone.push(gone.length + 1); if (gone.length === 18) await finalWeb.promise; } });
  vm.runInContext(between('async function runRecoveryCyclesFromIos(', 'async function main('), c);
  const page = { bringToFront: async () => {}, locator: () => ({ waitFor: async () => {} }) };
  const body = c.runIosInitiatorAndRecovery(page, ios, 'controlled-existing-android');
  void body.then(() => { finished = true; });
  for (let step = 0; step < 100 && gone.length !== 18; step++) await settle();
  assert.equal(gone.length, 18); assert.equal(completeCalls, 0); assert.equal(finished, false);
  assert.equal(allows.filter(([kind]) => kind === 'ios-sender').length, 18);
  for (const prefix of ['IOS_RECOVERY_REQUEST_SENT_', 'IOS_RECOVERY_SECRET_VISIBLE_', 'IOS_RECOVERY_CLOSED_']) {
    assert.equal(waits.filter(marker => marker.startsWith('E2E: ' + prefix)).length, 18);
  }
  finalWeb.resolve(); await settle(); assert.equal(completeCalls, 1); assert.equal(finished, false);
  iosJoin.resolve(); await settle(); assert.equal(finished, false, 'iOS join cannot substitute for Android natural result');
  androidResult.resolve({ stdout: 'strict existing Android result', stderr: '' });
  await body; assert.equal(finished, true);
});

test('actual Test2 first failure preserves screenshot before browser close and cancels the losing business body', async t => clocked(t, async () => {
  const gate = deferred(), entered = deferred(), events = [], sends = [];
  const ios = { assertBusinessActive() {}, waitForMarker: async () => {} };
  const android = { waitForMarker: async () => {} };
  const primary = new Error('controlled first failure');
  const c = businessContext({ processes: [], ownedChildClosures: new Map(), iosSuiteActors: [], watchedProcessOutputs: [],
    createHarnessShutdown, activeBrowserPage: { screenshot: async () => { events.push('screenshot'); } },
    browserInstance: { close: async () => { events.push('browser-close'); gate.resolve(); } },
    e2eRoot: '/controlled/e2e', serverContainer: 'controlled-test2-server', mkdirSync() {}, activeAndroidSerial: undefined,
    androidRecoveryDiagnostics: { stateSequence: 0, lines: [] },
    approvalCoordinator: { allow(kind, number) { if (kind === 'ios-sender') sends.push(number); }, close: async () => {} },
    waitWebIncomingGone: async () => { entered.resolve(); await gate.promise; },
    dumpWatchedProcessTails() {}, printFailureDiagnostics() {}, collectXcresultDiagnostics: async () => { events.push('diagnostics'); },
    runAndWait: async () => { events.push('cleanup'); } });
  vm.runInContext(between('async function captureWebFailureArtifact(', 'function printFailureDiagnostics('), c);
  vm.runInContext(between('async function performCleanup(', 'async function findIosSimulator('), c);
  vm.runInContext(between('async function runRecoveryCyclesFromIos(', 'async function runIosInitiatorAndRecovery('), c);
  c.nativeBusinessPromise = c.runRecoveryCyclesFromIos({}, ios, android);
  void c.nativeBusinessPromise.catch(() => {}); await entered.promise;
  await vm.runInContext('harnessShutdown.finish({ error: controlledPrimary, diagnostics: true })',
    Object.assign(c, { controlledPrimary: primary }));
  await assert.rejects(c.nativeBusinessPromise, error => error === primary);
  assert.deepEqual(sends, [1], 'no next approval cycle may start after first failure');
  assert(events.indexOf('screenshot') < events.indexOf('browser-close'));
  assert(events.indexOf('browser-close') < events.indexOf('diagnostics'));
  assert.equal(events.filter(event => event === 'screenshot').length, 1);
  assert.equal(events.filter(event => event === 'cleanup').length, 1);
  assert.throws(() => vm.runInContext('harnessShutdown.assertCanSpawn()', c), /shutdown is already in progress/);
}));
