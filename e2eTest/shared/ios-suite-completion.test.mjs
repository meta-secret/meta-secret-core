import assert from 'node:assert/strict';
import test from 'node:test';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// The private RED file may live outside e2eTest. Consume the actual public export,
// never a copied actor body; the owning driver binds this cwd/module hash.
const { createIosSelectedSuiteActor } = await import(
  pathToFileURL(resolve(process.cwd(), 'shared/ios-selected-suite.mjs')).href,
);

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
};
const settle = async () => { for (let i = 0; i < 20; i += 1) await Promise.resolve(); };

function watchedFixture() {
  const result = deferred();
  const closed = deferred();
  const markers = new Map();
  const signals = [];
  let didClose = false;
  const child = { exitCode: null, signalCode: null, kill(signal) {
    signals.push(signal);
    child.signalCode = signal;
    result.reject(new Error(`fixture wrapper closed by ${signal}`));
    didClose = true;
    closed.resolve({ code: null, signal });
    return true;
  } };
  return {
    signals, markerCount: () => markers.size, isClosed: () => didClose,
    watched: {
      child, result: result.promise, closed: closed.promise,
      waitForMarker(marker, deadline) {
        assert(deadline === null || (Number.isFinite(deadline) && deadline > 0));
        const wait = deferred();
        const timer = deadline === null ? null : setTimeout(() => {
          markers.delete(marker);
          wait.reject(new Error(`Timed out waiting for ${marker}`));
        }, deadline);
        markers.set(marker, { ...wait, timer });
        return wait.promise;
      },
      cancelPendingWaits(error) {
        for (const wait of markers.values()) {
          if (wait.timer !== null) clearTimeout(wait.timer);
          wait.reject(error);
        }
        markers.clear();
      },
      outputTail: () => 'controlled nonsensitive fixture output',
    },
  };
}

test('explicit suite completion does not time out during business after early result subscription', { timeout: 1_000 }, async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const fakeSetTimeout = globalThis.setTimeout;
  const fakeClearTimeout = globalThis.clearTimeout;
  const pendingTimers = new Set();
  // Track the real actor's mock-clock timers as well as fixture marker timers.
  // These wrappers only account for handles; Node's mock clock owns scheduling.
  globalThis.setTimeout = (callback, delay, ...args) => {
    let handle;
    handle = fakeSetTimeout(() => { pendingTimers.delete(handle); callback(...args); }, delay);
    pendingTimers.add(handle);
    return handle;
  };
  globalThis.clearTimeout = handle => { pendingTimers.delete(handle); fakeClearTimeout(handle); };
  const fixture = watchedFixture();
  let actor, primary, failure, beforeCleanup;
  let outcome = 'pending';
  let timersBeforeRepair = null;
  const cleanupErrors = [];
  try {
    actor = createIosSelectedSuiteActor({ watched: fixture.watched, selector: 'Fixture/phaseBoundary',
      timeoutMs: 600, deferCompletionTimeout: true,
      naturalExitMs: 5, closeTimeoutMs: 5, killTimeoutMs: 2,
      onPassed: () => assert.fail('This observation-only case never emits a passing suite'),
    });
    void actor.result.then(() => { outcome = 'resolved'; }, error => { outcome = 'rejected'; failure = error; });
    const joinMarker = deferred();
    const initialJoin = Promise.race([
      joinMarker.promise,
      actor.result.then(() => { throw new Error('fixture actor finished before join marker'); }),
    ]);
    joinMarker.resolve();
    await initialJoin; // Exact early .result subscription shape from Test1, no completion handoff.
    t.mock.timers.tick(601);
    await settle();
    beforeCleanup = outcome;
    assert.equal(outcome, 'pending',
      'Suite completion timeout must not poison business after early result subscription');
    // No complete() call: this first semantic assertion cannot fail on a missing future API.
  } catch (error) { primary = error; }
  finally {
    try { if (actor) await actor.cancelAndJoin(); }
    catch (error) { cleanupErrors.push(error); }
    await settle();
    timersBeforeRepair = pendingTimers.size;
    if (timersBeforeRepair !== 0) cleanupErrors.push(new Error('Actor left mock timer handles after cancelAndJoin'));
    if (fixture.markerCount() !== 0) cleanupErrors.push(new Error('Actor left marker observations after cancelAndJoin'));
    if (!fixture.isClosed()) cleanupErrors.push(new Error('Exact fake wrapper did not close'));
    // Repair only after recording a cleanup failure; it cannot certify this case as eligible RED.
    for (const handle of [...pendingTimers]) clearTimeout(handle);
    globalThis.setTimeout = fakeSetTimeout;
    globalThis.clearTimeout = fakeClearTimeout;
    t.mock.timers.reset();
  }
  console.log('R18_SUITE_COMPLETION_FIXTURE ' + JSON.stringify({
    version: 1, earlyResultSubscribed: true, completionCalled: false,
    businessElapsedMs: 601, completionBoundMs: 600,
    outcomeBeforeCleanup: beforeCleanup,
    prematureFailureWasMarkerTimeout: Boolean(failure?.message.startsWith('Timed out waiting for Test Suite')),
    fakeChildClosed: fixture.isClosed(), signals: fixture.signals,
    pendingMarkerCount: fixture.markerCount(), timersBeforeRepair,
    pendingTimerCount: pendingTimers.size, cleanupErrorCount: cleanupErrors.length,
  }));
  if (cleanupErrors.length) throw new AggregateError(primary ? [primary, ...cleanupErrors] : cleanupErrors,
    'Fixture cleanup failed; this is not an eligible semantic RED', { cause: primary });
  if (primary) throw primary;
});


import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { EventEmitter } from 'node:events';
const { createHarnessShutdown, formatHarnessShutdownError } = await import(pathToFileURL(resolve(process.cwd(), 'shared/harness-shutdown.mjs')).href);
const { markerScan, markerScanStream } = await import(pathToFileURL(resolve(process.cwd(), 'shared/process-marker.mjs')).href);
const { createDiagnosticRedactor } = await import(pathToFileURL(resolve(process.cwd(), 'shared/diagnostic-redaction.mjs')).href);
const source = file => readFileSync(resolve(process.cwd(), file), 'utf8');
const between = (text, first, last) => {
  const start = text.indexOf(first), end = text.indexOf(last, start + first.length);
  assert(start >= 0 && end > start, 'Actual owning declaration boundary missing');
  return text.slice(start, end);
};
async function clocked(t, work) {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const originalSet = globalThis.setTimeout, originalClear = globalThis.clearTimeout, handles = new Set(), actors = [];
  globalThis.setTimeout = (callback, duration, ...args) => {
    let handle; handle = originalSet(() => { handles.delete(handle); callback(...args); }, duration); handles.add(handle); return handle;
  };
  globalThis.clearTimeout = handle => { handles.delete(handle); originalClear(handle); };
  let primary; const cleanup = [];
  try { await work({ actors, tick: async ms => { t.mock.timers.tick(ms); await settle(); }, handles }); }
  catch (error) { primary = error; }
  finally {
    let joined;
    void Promise.allSettled(actors.map(actor => actor.cancelAndJoin())).then(results => { joined = results; });
    // Advance each newly armed mock close stage after its microtasks, also on a failed assertion.
    for (let step = 0; step < 10 && !joined; step++) { await settle(); t.mock.timers.tick(50); }
    await settle();
    if (!joined) cleanup.push(new Error('Owned fixture joins did not settle within bounded mock close stages'));
    for (const result of joined ?? []) if (result.status === 'rejected') cleanup.push(result.reason);
    if (handles.size) cleanup.push(new Error('Mock timer handles remained after owned joins'));
    for (const handle of [...handles]) clearTimeout(handle);
    globalThis.setTimeout = originalSet; globalThis.clearTimeout = originalClear; t.mock.timers.reset();
  }
  if (cleanup.length) throw new AggregateError(primary ? [primary, ...cleanup] : cleanup, 'Host fixture did not drain', { cause: primary });
  if (primary) throw primary;
}
function richFixture({ ignoreTerm = false, autoBusiness = false } = {}) {
  const raw = deferred(), closed = deferred(), waits = new Map(), seen = new Set(), signals = [], deadlines = [];
  let error; const child = new EventEmitter(); child.stdout = new EventEmitter(); child.stderr = new EventEmitter();
  Object.assign(child, { pid: 51001, exitCode: null, signalCode: null, kill(signal) {
    signals.push(signal); if (ignoreTerm && signal === 'SIGTERM') return true;
    finish(null, signal, new Error(`Controlled process signal ${signal}`)); return true;
  } });
  function cancel(failure) { error ??= failure; for (const [marker, wait] of waits) { clearTimeout(wait.timer); wait.reject(error); waits.delete(marker); } }
  function finish(code, signal, failure) {
    child.exitCode = code; child.signalCode = signal;
    cancel(failure ?? new Error('Controlled process ended before expected marker'));
    if (failure) raw.reject(failure); else raw.resolve({ stdout: '', stderr: '' });
    child.emit('exit', code, signal); child.emit('close', code, signal); closed.resolve({ code, signal });
  }
  const watched = { child, result: raw.promise, closed: closed.promise,
    waitForMarker(marker, duration = 300_000) {
      deadlines.push({ marker, duration });
      if (seen.has(marker)) return Promise.resolve();
      if (error) return Promise.reject(error);
      if (autoBusiness && marker.startsWith('E2E:')) return Promise.resolve();
      const wait = deferred(); wait.timer = duration === null ? null : setTimeout(() => {
        waits.delete(marker); wait.reject(new Error(`Timed out waiting for ${marker}`));
      }, duration); waits.set(marker, wait); return wait.promise;
    }, cancelPendingWaits: cancel, outputTail: () => 'controlled nonsensitive output' };
  return { watched, signals, deadlines, waits, finish,
    emit(marker) { seen.add(marker); const wait = waits.get(marker); if (wait) { clearTimeout(wait.timer); waits.delete(marker); wait.resolve(); } },
  };
}
function actorFor(clock, fixture, options = {}) {
  const actor = createIosSelectedSuiteActor({ watched: fixture.watched, selector: 'Fixture/exactActor', timeoutMs: 600,
    deferCompletionTimeout: true, naturalExitMs: 5, closeTimeoutMs: 5, killTimeoutMs: 2, ...options });
  clock.actors.push(actor); return actor;
}

test('explicit completion arms one unchanged deadline only at the final phase', async t => clocked(t, async clock => {
  const f = richFixture(), a = actorFor(clock, f); let failed = false; void a.result.catch(() => { failed = true; });
  await clock.tick(601); assert.equal(failed, false); assert(f.deadlines.every(row => row.duration === null));
  const first = a.complete(); assert.equal(a.complete(), first);
  await clock.tick(599); assert.equal(failed, false);
  await clock.tick(1); await assert.rejects(first, /Timed out waiting for Test Suite/);
  assert.deepEqual(f.signals, ['SIGTERM']); assert.equal(f.waits.size, 0);
}));
test('cached selected pass before complete retains strict natural close without another timer', async t => clocked(t, async clock => {
  const f = richFixture(); let passed = 0;
  const a = actorFor(clock, f, { onPassed: () => { assert.equal(f.watched.child.exitCode, 0); passed++; } });
  f.emit("Test Suite 'Selected tests' passed"); f.finish(0, null); await a.result;
  assert.equal(a.complete(), a.result); assert.equal(passed, 1); assert.equal(clock.handles.size, 0); assert.deepEqual(f.signals, []);
}));
test('selected failure is immediate during business while original bounded TERM KILL joins the owner', async t => clocked(t, async clock => {
  const f = richFixture({ ignoreTerm: true }), a = actorFor(clock, f);
  const missing = f.watched.waitForMarker('E2E: still-pending', 600); const cancelled = assert.rejects(missing, e => e.code === 'IOS_SELECTED_SUITE_FAILED');
  f.emit("Test Suite 'Selected tests' failed");
  await assert.rejects(a.failure, e => e.code === 'IOS_SELECTED_SUITE_FAILED'); await cancelled;
  assert.equal(f.watched.child.signalCode, null); await clock.tick(5); assert.deepEqual(f.signals, ['SIGTERM']);
  await clock.tick(5); await assert.rejects(a.result, e => e.code === 'IOS_SELECTED_SUITE_FAILED'); assert.deepEqual(f.signals, ['SIGTERM', 'SIGKILL']);
}));
test('actual process error nonzero and unexpected signal fail before completion is armed', async t => clocked(t, async clock => {
  for (const mode of ['error', 'nonzero', 'signal']) {
    const f = richFixture(), a = actorFor(clock, f), error = new Error(`Controlled ${mode}`);
    f.finish(mode === 'nonzero' ? 65 : mode === 'error' ? -1 : null, mode === 'signal' ? 'SIGABRT' : null, error);
    await assert.rejects(a.failure, e => e === error); await assert.rejects(a.result, e => e === error); assert.deepEqual(f.signals, []);
  }
}));
test('natural process zero without the selected suite marker remains failure', async t => clocked(t, async clock => {
  const f = richFixture(), a = actorFor(clock, f); f.finish(0, null);
  await assert.rejects(a.result, /ended before expected marker/); await assert.rejects(a.complete(), /ended before expected marker/);
}));
test('selected pass cannot hide a subsequent natural writer failure or signal', async t => clocked(t, async clock => {
  for (const signal of [null, 'SIGABRT']) {
    const f = richFixture(), a = actorFor(clock, f), failure = new Error('Controlled result writer failure');
    f.emit("Test Suite 'Selected tests' passed"); await settle(); f.finish(signal ? null : 65, signal, failure);
    await assert.rejects(a.result, e => e === failure); assert.deepEqual(f.signals, []);
  }
}));
test('cancel before final completion rejects observations and closes the exact fake child without leaks', async t => clocked(t, async clock => {
  const f = richFixture(), a = actorFor(clock, f); await a.cancelAndJoin();
  await assert.rejects(a.result, /cancelled before/); assert.equal(f.waits.size, 0); assert.equal(clock.handles.size, 0);
  assert.deepEqual(f.signals, ['SIGTERM']); assert.equal(a.complete(), a.result);
}));
test('default mode retains eager same deadline and does not gain an untimed lifetime', async t => clocked(t, async clock => {
  const f = richFixture(), a = actorFor(clock, f, { deferCompletionTimeout: false });
  assert(f.deadlines.every(row => row.duration === 600)); await clock.tick(600);
  await assert.rejects(a.result, /Timed out waiting/); assert.deepEqual(f.signals, ['SIGTERM']);
}));

function callerContext(extra = {}) {
  const context = { Promise, AggregateError, Error, Buffer, Set, Map, Date, resolve, setTimeout, clearTimeout, performance,
    console: { log() {}, error() {} }, process: { env: {}, stdout: { write() {} }, stderr: { write() {} } },
    scenario: { secret: { name: 'controlled-item', value: 'controlled-value' } }, recoveryShowTimeoutMs: 45_000,
    watchedProcessOutputs: [], createIosSelectedSuiteActor, createHarnessShutdown, formatHarnessShutdownError, markerScan, markerScanStream,
    createMatchedMarkerChoices: () => ({ push() {}, fail() {}, wait: () => assert.fail('Unexpected choice port') }),
    diagnosticRedactor: createDiagnosticRedactor(), appendBounded: (old, next) => (old + next).slice(-128_000),
    filterVerboseWatchedOutput: (_command, _sink, text) => text, appendFileSync() {}, diagnosticLogPath: 'controlled.log',
    redactCommandArguments: a => a, redactEnvironment: e => e, maxWatchedOutputChars: 128_000, writeDiagnostic() {},
    ...extra };
  return vm.createContext(context);
}
test('actual three caller watchers own null deadline observations and strict markerless process exit', async t => clocked(t, async clock => {
  for (const number of [1, 3, 4]) {
    const f = richFixture(), text = source(`Test${number}/orchestrator.mjs`);
    const c = callerContext({ run: () => f.watched.child });
    vm.runInContext(between(text, 'function watchProcessOutput(', number === 4 ? 'function watchAndroidInstrumentation(' : 'function printFailureDiagnostics('), c);
    const watched = c.watchProcessOutput('xcodebuild', [], {}); void watched.result.catch(() => {});
    const pending = watched.waitForMarker('required-marker', null); const refused = assert.rejects(pending, /ended before expected E2E event/);
    assert.equal(clock.handles.size, 0); f.finish(0, null); await refused; await watched.closed;
  }
}));
test('actual caller start bridges keep original timeout choices and only explicit full recovery opts in', async t => clocked(t, async clock => {
  for (const number of [1, 3]) {
    const f = richFixture(); const calls = []; const text = source(`Test${number}/orchestrator.mjs`);
    const c = callerContext({ watchedProcessOutputs: [], batchProducts: {}, coreRoot: '/controlled/core', composeRoot: '/controlled/compose',
      iosDerivedDataPath: '/controlled/derived', iosResultBundlePath: '/controlled/result', iosIdentity: { context: { runRoot: '/controlled/run' } },
      recoveryCycles: Array(18), iosRecoveryApprovalCycles: [], androidRecoveryApprovalCycles: [], actorEnvironment: {}, namespaceRedactor: createDiagnosticRedactor(),
      assertBatchProductsCurrent() {}, verifyIosXcodeBuild() {}, iosGeneratedTemplateProof: () => ({}),
      prepareScopedIosInvocation: () => ({ xctestrun: '/controlled/invocation' }), iosSerialTestArguments: () => ['exact-owned-selector'],
      scenario: { vault: { name: 'controlled-vault' }, secret: { name: 'controlled-item', value: 'controlled-value' } },
      watchProcessOutput: () => f.watched, iosSuiteActors: [],
      createIosSelectedSuiteActor: options => { calls.push(options); return createIosSelectedSuiteActor({ ...options, naturalExitMs: 5, closeTimeoutMs: 5, killTimeoutMs: 2 }); } });
    vm.runInContext(between(text, 'function startIosJoinTest(', 'async function prepareAndroidEmulator('), c);
    const actor = c.startIosJoinTest('controlled-existing-target'); clock.actors.push(actor);
    assert.equal(calls[0].timeoutMs, 600_000); assert.equal(calls[0].deferCompletionTimeout, true); assert(f.deadlines.every(row => row.duration === null));
  }
  for (const count of [6, 18]) for (const explicit of [false, true]) {
    const f = richFixture(), calls = [], text = source('Test4/orchestrator.mjs');
    const c = callerContext({ watched: f.watched, recoveryCycles: Array(count), testClass: 'Controlled', testMethod: 'selected',
      coreRoutingVerified: Promise.resolve(), ownedResultBundlePath: 'controlled-result', deferCompletionTimeout: explicit, recoveryPhase: undefined, test22ShowPhase: undefined, test25ShowPhase: undefined, recoveryShowPhase: undefined,
      removePassedIosTestResults() {}, createIosSelectedSuiteActor: options => { calls.push(options); return createIosSelectedSuiteActor({ ...options, naturalExitMs: 5, closeTimeoutMs: 5, killTimeoutMs: 2 }); } });
    const body = between(text, '  const selectedSuiteTimeoutMs =', '\n}\n\nasync function prepareAndroidEmulator(');
    vm.runInContext('function actualBridge() {\n' + body + '\n}', c); const actor = c.actualBridge(); clock.actors.push(actor);
    assert.equal(calls[0].timeoutMs, count === 18 ? 600_000 : 360_000); assert.equal(calls[0].deferCompletionTimeout, explicit);
  }
}));

test('actual Test1 and Test3 bodies retain all original eighteen cycles beyond startup allowance', async t => clocked(t, async clock => {
  for (const number of [1, 3]) {
    const f = richFixture({ autoBusiness: true }), a = actorFor(clock, f, { timeoutMs: 600_000 });
    const ios = { ...f.watched, ...a }, waits = [], android = { result: Promise.resolve(), waitForMarker: async marker => { waits.push(marker); } };
    let requests = 0, completed = 0, approvals = 0;
    const c = callerContext({ scenario: { secret: { name: 'controlled-item', value: 'controlled-value' }, recovery: JSON.parse(source(`Test${number}/scenario.json`)).recovery },
      page: { getByTestId: () => ({ click: async () => {} }) }, openWebSecrets: async () => {}, closeWebSecret: async () => {},
      approveJoinRequestOnWeb: async () => {}, waitWebSecretValue: async () => {}, startAndroidJoinTest: async () => android,
      startWebLiveboardRecovery: async () => { requests++; await clock.tick(40_000); },
      approvalCoordinator: { allow(kind) { if (number === 3 && kind === 'android-sender') { requests++; t.mock.timers.tick(40_000); } else approvals++; } },
      openWebIncoming: async () => ({ requestId: 'controlled-request' }), decideWebIncoming: async () => { approvals++; }, waitWebIncomingGone: async () => {} });
    const text = source(`Test${number}/orchestrator.mjs`);
    vm.runInContext(between(text, 'const recoveryCycles =', 'const recoveryShowTimeoutMs ='), c);
    const recovery = number === 1 ? 'async function runRecoveryCycles(' : 'async function runRecoveryCyclesFromAndroid(';
    vm.runInContext(between(text, recovery, 'async function main()'), c);
    ios.complete = () => {
      assert.equal(requests, 18);
      assert.equal(f.deadlines.filter(row => /IOS_RECOVERY_REQUEST_CLOSED_/.test(row.marker)).length, 18);
      completed++; f.emit("Test Suite 'Selected tests' passed"); f.finish(0, null); return a.complete();
    };
    await c.runIosJoinAndRecovery(c.page, ios, number === 1 ? 'controlled-existing-android' : android);
    assert.equal(requests, 18); assert.equal(approvals, 18); assert.equal(completed, 1); assert.equal(clock.handles.size, 0);
  }
}));
async function actualCancellationControl(clock, variant, mode = 'join') {
    const number = variant === 'offline' ? 4 : variant;
    const f = richFixture({ autoBusiness: true }), a = actorFor(clock, f), ios = { ...f.watched, ...a };
    const gate = deferred(), release = deferred(), entered = deferred(), browserRelease = deferred(); const events = []; let phaseEnded = false;
    const android = { result: Promise.resolve(), waitForMarker: async () => {} };
    let approvals = 0, cycles = 0, spawns = 0;
    const heldWeb = async () => { entered.resolve(); try { return await gate.promise; } finally { await release.promise; events.push('loser-settled'); } };
    const page = { getByTestId: () => ({ click: async () => {} }), evaluate: heldWeb };
    const browser = { close: async () => { events.push('browser-close-started'); if (mode === 'browser-stalled') await browserRelease.promise;
      events.push('browser-closed'); gate.resolve({ operations: [], secrets: [] }); } };
    const c = callerContext({ page, androidTest: android, androidSerial: 'controlled-android', simulatorUdid: 'controlled-ios',
      testNumber: number, scenarioArtifactLabel: 'scenario', requestedSignalExitCode: undefined,
      processes: [f.watched.child], ownedChildClosures: new Map([[f.watched.child, f.watched.closed]]), iosSuiteActors: [ios],
      browserInstance: browser, nativeBusinessPromise: null, nativeBusinessFailure: null, genericIosActor: null, genericIosBusinessPromise: null,
      startIosJoinTest: () => ios, startAndroidJoinTest: async () => android, createServer() {},
      spawn: () => { spawns++; return f.watched.child; }, projectRoot: '/controlled/project', androidSdkTools: {}, withAndroidSdkPath: value => value,
      openWebSecrets: async () => {}, closeWebSecret: async () => {}, waitWebSecretValue: async () => {}, approveJoinRequestOnWeb: async () => {},
      recoveryCycles: [{ number: 1, approver: number === 3 ? 'web' : 'ios', senderSecrets: {}, approvals: [], senders: [] }],
      startWebLiveboardRecovery: heldWeb, openWebIncoming: heldWeb, decideWebIncoming: async () => { approvals++; }, waitWebIncomingGone: async () => {},
      approvalCoordinator: { allow() { approvals++; } }, waitForWebSecrets: async () => {}, concurrentFifoIdentities: async () => ({}),
      setCurrentCycleForDiagnostics() { cycles++; }, waitForFifoCanonicalSnapshot() {},
      watchedProcessOutputs: [f.watched], performCleanup: async () => { events.push('cleanup'); }, printFailureDiagnostics() {}, captureFailureScreenshots: async () => {},
      ownedProcessTreePids: async () => [f.watched.child.pid], cancelledActorPids: [],
      terminateOwnedProcessTree: async () => { events.push('owned-tree-closed'); }, networkGateProcesses: new Map() });
    if (variant === 'offline') {
      c.scenario.offlineReceiver = { platform: 'android' };
      c.recoveryCycles = ['android', 'web', 'ios'].map((platform, i) => ({ number: i + 1, offlineReceiver: platform,
        senderSecrets: { web: 'controlled-item', android: 'controlled-item' }, approvals: [{ platform: 'ios' }] }));
      Object.defineProperty(android, 'result', { get: heldWeb });
    }
    const text = source(`Test${number}/orchestrator.mjs`);
    if (number !== 4) {
      vm.runInContext(between(text, 'function run(command,', 'function runAndWait('), c);
      vm.runInContext(between(text, number === 1 ? 'async function runRecoveryCycles(' : 'async function runRecoveryCyclesFromAndroid(', 'async function main()'), c);
    } else {
      c.process.kill = () => assert.fail('No sibling process in this exact fixture');
      // Consume the existing first-failure latch and no-drain guard, not a model
      // that skips the real generic failure/cancellation boundary.
      vm.runInContext(between(text, 'let genericFirstFailure;', 'async function readFifoFailureCurrent('), c);
      vm.runInContext(between(text, 'async function runOfflineReceiverRecovery(', 'async function createWebInitiatedVault('), c);
      vm.runInContext(between(text, 'async function runConcurrentRecoveryCycles(', 'async function main()'), c);
    }
    vm.runInContext(between(text, 'const harnessShutdown = createHarnessShutdown(', 'function stopProcesses('), c);
    const first = number === 4 ? '  const iosTest = genericIosActor = startIosJoinTest' : '  const iosTest = startIosJoinTest';
    const last = number === 4 ? '\n  console.log(`✅ Test #${testNumber} concurrent' : "\n  console.log('✅ Test #";
    const main = text.slice(text.indexOf('async function main()'));
    vm.runInContext('async function actualNativePhase() {\n' + between(main, first, last) + '\n}', c);
    const phase = c.actualNativePhase(); void phase.finally(() => { phaseEnded = true; }).catch(() => {});
    try {
      await entered.promise; const beforeFailureApprovals = approvals;
      f.emit("Test Suite 'Selected tests' failed"); await settle(); await clock.tick(50);
      assert.equal(phaseEnded, false, 'Owning phase cannot silently abandon its still-held losing body');
      if (number !== 4) assert.throws(() => c.run('controlled-late-spawn', []), /shutdown is already in progress/);
      if (mode === 'join') {
        release.resolve(); await assert.rejects(phase, error => error.code === 'IOS_SELECTED_SUITE_FAILED');
        assert(events.indexOf('loser-settled') < events.indexOf('cleanup'));
      } else {
        await clock.tick(5_001); await clock.tick(5_001);
        await assert.rejects(phase, error => {
          assert.equal(error.cause.code, 'IOS_SELECTED_SUITE_FAILED');
          const retained = formatHarnessShutdownError(error, value => value);
          assert.match(retained, /Owned .*business continuation remains unjoined after 5000ms/);
          if (mode === 'browser-stalled') assert.match(retained, /Owned browser close remains unproven after 5000ms/);
          assert(events.includes('cleanup'), 'Explicit unresolved blocker must not prevent actual final cleanup');
          if (number === 4) assert(events.includes('owned-tree-closed'), 'Tree drain precedes stalled browser join');
          return true;
        });
        assert(!events.includes('loser-settled'), 'Failed teardown must not claim the loser was joined');
      }
      assert.equal(approvals, beforeFailureApprovals); assert.equal(spawns, 0); assert(cycles <= 1);
      assert.equal(f.watched.child.signalCode, 'SIGTERM');
    } finally {
      browserRelease.resolve(); gate.resolve({ operations: [], secrets: [] }); release.resolve();
      await a.cancelAndJoin(); await settle();
      await (number === 4 ? c.genericIosBusinessPromise : c.nativeBusinessPromise)?.catch(() => {});
    }
}
test('actual caller cancellation joins losing bodies and forbids later approval cycle or spawn', async t => clocked(t, async clock => {
  for (const variant of [1, 3, 4, 'offline']) await actualCancellationControl(clock, variant);
}));
test('actual stalled browser and losing-body teardown fail finitely without concealing the primary', async t => clocked(t, async clock => {
  for (const variant of [1, 3, 4]) for (const mode of ['browser-stalled', 'body-stalled']) await actualCancellationControl(clock, variant, mode);
}));

test('actual offline terminal boundary arms the same completion allowance before awaiting its live wrapper', async t => clocked(t, async clock => {
  const f = richFixture({ autoBusiness: true }), actor = actorFor(clock, f), events = [];
  let completionCalls = 0, completed = false;
  const ios = { ...f.watched, ...actor, complete() { completionCalls++; return actor.complete(); } };
  const android = { result: Promise.resolve(), waitForMarker: async () => {} };
  const c = callerContext({ recoveryCycles: ['android', 'web', 'ios'].map((platform, index) => ({ number: index + 1,
      offlineReceiver: platform, senderSecrets: { web: 'controlled-item', android: 'controlled-item' }, approvals: [{ platform: 'ios' }, { platform: 'web' }] })),
    setCurrentCycleForDiagnostics() {}, stopAndroidApplication: async () => {}, startWebRecovery: async () => {},
    revealAndCloseWebSecret: async () => {}, runAndroidStaleAlertAssertion: async () => {}, stopWebApplication: async () => {},
    startAndroidRemainingRecoveryTest: async () => android, reopenWebAfterOffline: async () => {}, assertNoStaleWebRecoveryAlert: async () => {},
    stopIosApplication: async () => { events.push('ios-stopped'); }, approvalCoordinator: { allow(kind) { events.push(kind); } },
    approveIncomingRecoveryOnWeb: async () => { events.push('late-web-approval'); }, runIosStaleAlertAssertion: async () => { events.push('new-native-actor'); } });
  const text = source('Test4/orchestrator.mjs');
  vm.runInContext(between(text, 'async function runOfflineReceiverRecovery(', 'async function createWebInitiatedVault('), c);
  const body = c.runOfflineReceiverRecovery({}, ios, android, 'controlled-android', 'controlled-ios');
  void body.finally(() => { completed = true; }).catch(() => {});
  for (let step = 0; step < 10 && !completionCalls; step++) await settle();
  assert.equal(completionCalls, 1); assert(events.includes('ios-offline-stop')); assert.equal(completed, false);
  await clock.tick(599); assert.equal(completed, false);
  await clock.tick(1); await assert.rejects(body, /Timed out waiting for Test Suite/);
  assert(!events.includes('late-web-approval')); assert(!events.includes('new-native-actor'));
  assert.equal(ios.complete(), actor.result); assert.equal(clock.handles.size, 0);
}));
test('actual outer primary and cleanup catch boundaries retain distinct causes through existing redaction', async () => {
  const canary = 'CONTROLLED_PRIVATE_CANARY';
  for (const number of [1, 3, 4]) {
    const logs = [], primary = new Error('PRIMARY_A ' + canary), cleanup = new Error('CLEANUP_B ' + canary);
    const outer = new AggregateError([primary, cleanup], 'Native business failed and cancellation failed', { cause: primary });
    const redactor = createDiagnosticRedactor([canary]);
    const c = callerContext({ main: () => Promise.reject(outer), testNumber: number, requestedSignalExitCode: undefined,
      errorRedactor: redactor, redactSensitiveText: redactor.redactText, printFailureDiagnostics() {},
      stopProcesses: async () => { throw cleanup; }, harnessShutdown: { finish: async () => { throw cleanup; } },
      writeDiagnostic: text => logs.push(text), console: { log() {}, error: text => logs.push(text) },
      process: { exit(code) { assert.equal(code, 1); } } });
    const text = source(`Test${number}/orchestrator.mjs`);
    await vm.runInContext(text.slice(text.lastIndexOf('main().catch(async (error) => {')), c);
    const retained = logs.join('\n'); assert.match(retained, /PRIMARY_A/); assert.match(retained, /CLEANUP_B/);
    assert(!retained.includes(canary)); assert.match(retained, /REDACTED/);
  }
});
