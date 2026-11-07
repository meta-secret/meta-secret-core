// HOST ONLY. Actual candidate imports and real loopback/owned-worker API tests.
// Synthetic contexts/workers below are not native Android/iOS or UI acceptance.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createServer} from 'node:http';
import {once} from 'node:events';
import {spawn} from 'node:child_process';
import {createTest24RecoveryPhase, createRecoveryPhaseRegistry, handleRecoveryPhaseRequest,
  abortRecoveryPhaseHarness} from './recovery-phase.mjs';
import {createHarnessShutdown} from './harness-shutdown.mjs';
import {superviseAndroidInstrumentation} from './android-instrumentation.mjs';
import {createIosSelectedSuiteActor} from './ios-selected-suite.mjs';

const binding = () => ({version: 1, runId: 'SYNTHETIC_UNIT_RUN', testNumber: 24, attempt: 1,
  blockId: 'secret1-three-device', cycle: 13, logicalStep: 1, mode: 'ios-multi-secret-lifecycle',
  contextId: 'a'.repeat(32), sender: 'web', participants: ['web', 'android', 'ios'].map((platform, i) => ({
    platform, role: platform === 'web' ? 'sender' : 'receiver', invocationId: String(i + 1).repeat(32)}))});
const query = (context, platform, view) => `/recovery-phase?v=1&context=${context.contextId}&participant=${context.participants.find(p => p.platform === platform).invocationId}&view=${view}`;
const marker = (context, platform, event) => {
  const text = `E2E: RECOVERY_PHASE_V1 context=${context.contextId} invocation=${context.participants.find(p => p.platform === platform).invocationId} platform=${platform} phase=${event}\n`;
  return platform === 'android' ? '11-03 00:15:35.985 18342 18362 I MetaSecretE2E: ' + text : text;
};
function fixture(options = {}) {
  let time = 0, sequence = 0;
  const timers = new Map(), scope = binding();
  const phase = createTest24RecoveryPhase({binding: scope, now: () => time,
    schedule: (callback, delay) => { const id = ++sequence; timers.set(id, {callback, at: time + delay}); return id; },
    cancel: id => timers.delete(id), ...options});
  const streams = Object.fromEntries(['android', 'ios'].map(p => [p, phase.observer(p, {})]));
  const ready = () => { phase.web('READY'); for (const p of ['android', 'ios']) streams[p].push('stdout', marker(scope, p, 'READY')); };
  const advance = value => { time = value; for (const [id, timer] of [...timers]) if (timer.at <= time && timers.delete(id)) timer.callback(); };
  return {phase, streams, ready, advance, scope};
}

test('real HTTP handler has immediate canonical JSON, duplicate query/method and abort refusals', async () => {
  const scope = binding(), registry = createRecoveryPhaseRegistry();
  const phase = registry.register(scope);
  const server = createServer((req, res) => { if (!handleRecoveryPhaseRequest(req, res, registry)) res.writeHead(404).end(); });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const url = `http://127.0.0.1:${server.address().port}`;
  try {
    const path = query(scope, 'android', 'request-sent');
    const waiting = await fetch(url + path, {signal: AbortSignal.timeout(2000)});
    assert.equal(waiting.status, 200); assert.equal(waiting.headers.get('content-type'), 'application/json');
    const text = await waiting.text();
    assert.deepEqual(Object.keys(JSON.parse(text)), ['version', 'contextId', 'invocationId', 'view', 'state', 'deadlineRemainingMs', 'badgeRemainingMs']);
    assert.equal(JSON.parse(text).state, 'WAITING'); assert.equal(JSON.parse(text).badgeRemainingMs, 0);
    for (const suffix of ['&v=1', '&participant=' + '2'.repeat(32), '&extra=1'])
      assert.equal((await fetch(url + path + suffix)).status, 400);
    assert.equal((await fetch(url + path, {method: 'POST'})).status, 400);
    registry.freeze(Error('synthetic terminal abort'));
    assert.equal((await fetch(url + path)).status, 410);
    assert.throws(() => phase.releaseSender());
  } finally { registry.freeze(Error('fixture cleanup')); await new Promise(resolve => server.close(resolve)); }
});

test('hard D fires without any HTTP response; post-abort action callback is never invoked', async () => {
  let failures = 0, effect = 0;
  const f = fixture({onAbort: () => { failures++; }});
  f.advance(600_000); assert.equal(failures, 1);
  await assert.rejects(f.phase.run(() => { effect++; })); assert.equal(effect, 0);
  assert.equal(f.phase.snapshot(query(f.scope, 'android', 'request-sent')).status, 408);
  f.advance(900_000); assert.equal(failures, 1);
});

test('hard B fires during a receiver UI wait, not only when another snapshot is requested', () => {
  let error;
  const f = fixture({onAbort: failure => { error = failure; }}); f.ready(); f.phase.releaseSender();
  f.advance(199); f.phase.web('REQUEST_SENT');
  f.advance(180_199); assert.equal(error.code, 'RECOVERY_PHASE_EXPIRED');
  assert.throws(() => f.streams.android.push('stdout', marker(f.scope, 'android', 'INCOMING_VISIBLE')));
});

test('both badge proofs close only B; original overall D still bounds approval/reveal work', () => {
  const f = fixture(); f.ready(); f.phase.releaseSender(); f.advance(1); f.phase.web('REQUEST_SENT');
  for (const p of ['android', 'ios']) f.streams[p].push('stdout', marker(f.scope, p, 'INCOMING_VISIBLE'));
  f.advance(200_000);
  assert.equal(f.phase.snapshot(query(f.scope, 'android', 'request-sent')).status, 410);
  assert.doesNotThrow(() => f.phase.requireBothBadges());
  f.advance(600_000); assert.throws(() => f.phase.assertActive());
});

test('malformed string/coercion fields and duplicate participant context never opt in', () => {
  for (const delta of [{runId: 1}, {blockId: 1}, {attempt: 0}, {attempt: 1000000000}, {testNumber: '24'}, {cycle: 13.2}, {logicalStep: true},
    {runId: 'SYNTHETIC_UNIT_RUN\n'}, {blockId: 'secret1-three-device\n'}, {contextId: 'a'.repeat(32) + '\n'}])
    assert.throws(() => createTest24RecoveryPhase({binding: {...binding(), ...delta}}));
  const scope = binding(); scope.participants[1].invocationId = scope.participants[0].invocationId;
  assert.throws(() => createTest24RecoveryPhase({binding: scope}));
  const newline = binding(); newline.participants[1].invocationId += '\n';
  assert.throws(() => createTest24RecoveryPhase({binding: newline}));
});

test('unregistered cycle and the same actual owner reused by a second platform are refused', () => {
  const registry = createRecoveryPhaseRegistry(), phase = registry.register(binding());
  assert.throws(() => registry.assertCycleActive(14));
  const owner = {}; phase.observer('android', owner);
  assert.throws(() => phase.observer('ios', owner));
  registry.freeze(Error('fixture cleanup'));
});

test('actual ordinary selected-suite receipt shortens only the same host monotonic D', async () => {
  const watched = {child: {exitCode: 0, signalCode: null}, closed: Promise.resolve(), result: Promise.resolve(),
    waitForMarker: marker => marker.includes('passed') ? Promise.resolve() : new Promise(() => {}),
    cancelPendingWaits() {}, outputTail: () => ''};
  const phase = createTest24RecoveryPhase({binding: binding()});
  const before = phase.deadlineAt;
  const actor = createIosSelectedSuiteActor({watched, selector: 'SyntheticHost/ordinary', timeoutMs: 600_000});
  const actual = actor.selectedSuiteDeadline;
  assert.equal(actual.timeoutMs, 600_000); phase.setIosDeadline(actual.deadlineAt);
  assert.equal(phase.deadlineAt, Math.min(before, actual.deadlineAt));
  phase.abort(Error('fixture done')); await actor.result;
});

test('actual phase abort bridge enters real harness shutdown and closes a tracked worker, not logcat alone', async () => {
  const worker = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], {stdio: 'ignore'});
  const closed = once(worker, 'close'); await once(worker, 'spawn');
  const effects = []; let logcatKilled = false;
  const actor = superviseAndroidInstrumentation({instrumentationResult: closed.then(() => ({stdout: 'HOST_WORKER_FIXTURE', stderr: ''})),
    logcat: {child: {killed: false, kill() { logcatKilled = true; this.killed = true; }}, result: Promise.resolve(),
      waitForMarker: () => new Promise(() => {})}, selector: 'SYNTHETIC_HOST_WORKER'});
  const shutdown = createHarnessShutdown({
    async cancelActors() { effects.push('cancel'); worker.kill('SIGTERM'); await closed; },
    async captureDiagnostics() { effects.push('diagnostics'); }, async cleanup() { effects.push('cleanup'); },
  });
  const coordinator = {freezeGrants() { effects.push('freeze'); }};
  const f = fixture({onAbort: abortRecoveryPhaseHarness({coordinator, shutdown})});
  try {
    actor.child.kill('SIGTERM'); assert.equal(logcatKilled, true); assert.equal(worker.exitCode, null);
    f.advance(600_000); await shutdown.finish(); await actor.result;
    assert.deepEqual(effects, ['freeze', 'cancel', 'diagnostics', 'cleanup']);
    assert.ok(worker.exitCode !== null || worker.signalCode !== null);
    assert.throws(() => shutdown.assertCanSpawn());
  } finally { if (worker.exitCode === null && worker.signalCode === null) worker.kill('SIGKILL'); await closed; }
});

test('public async waiters require all current READY, actual SENT and each receiver badge', {timeout: 2000}, async () => {
  const f = fixture(), resolved = new Set();
  try {
    const ready = f.phase.waitFor('READY', 240_000).then(() => resolved.add('ready'));
    f.phase.web('READY'); f.streams.android.push('stdout', marker(f.scope, 'android', 'READY'));
    await Promise.resolve(); assert.equal(resolved.size, 0);
    f.streams.ios.push('stdout', marker(f.scope, 'ios', 'READY'));
    await ready; assert.deepEqual([...resolved], ['ready']);
    f.phase.releaseSender();
    const sent = f.phase.waitFor('REQUEST_SENT', 180_000).then(() => resolved.add('sent'));
    const android = f.phase.waitFor('INCOMING_VISIBLE', 180_000, 'android').then(() => resolved.add('android'));
    const ios = f.phase.waitFor('INCOMING_VISIBLE', 180_000, 'ios').then(() => resolved.add('ios'));
    await Promise.resolve(); assert.deepEqual([...resolved], ['ready']);
    f.phase.web('REQUEST_SENT'); await sent;
    assert.deepEqual([...resolved], ['ready', 'sent']);
    f.streams.android.push('stdout', marker(f.scope, 'android', 'INCOMING_VISIBLE')); await android;
    assert.deepEqual([...resolved], ['ready', 'sent', 'android']);
    assert.throws(() => f.phase.requireBothBadges());
    f.streams.ios.push('stdout', marker(f.scope, 'ios', 'INCOMING_VISIBLE')); await ios;
    assert.deepEqual([...resolved], ['ready', 'sent', 'android', 'ios']);
    await f.phase.waitFor('READY', 240_000); await f.phase.waitFor('REQUEST_SENT', 180_000);
    await f.phase.waitFor('INCOMING_VISIBLE', 180_000, 'android');
    f.phase.requireBothBadges(); f.phase.complete();
  } finally { f.phase.abort(Error('fixture cleanup')); }
});

test('public waiters close together on readiness timeout, missing SENT or badge deadline without a new grant', {timeout: 2000}, async () => {
  for (const stage of ['readiness', 'missing-sent', 'missing-badge']) {
    let first, aborts = 0;
    const f = fixture({onAbort: error => { first = error; aborts++; }}), resolved = [];
    try {
      if (stage !== 'readiness') { f.ready(); f.phase.releaseSender(); }
      if (stage === 'missing-badge') { f.advance(10); f.phase.web('REQUEST_SENT'); }
      const event = stage === 'readiness' ? 'READY' : stage === 'missing-sent' ? 'REQUEST_SENT' : 'INCOMING_VISIBLE';
      const maximum = stage === 'readiness' ? 240_000 : 180_000;
      const pending = ['android', 'ios'].map(platform => f.phase.waitFor(event, maximum,
        event === 'INCOMING_VISIBLE' ? platform : undefined).then(() => resolved.push(platform)));
      const failures = pending.map(promise => assert.rejects(promise, error => error === first));
      const failedPhase = assert.rejects(f.phase.failure, error => error === first);
      const deadline = stage === 'readiness' ? 240_000 : stage === 'missing-sent' ? 180_000 : 180_010;
      f.advance(deadline - 1); await Promise.resolve(); assert.equal(resolved.length, 0); assert.equal(aborts, 0);
      f.advance(deadline); await Promise.all([...failures, failedPhase]);
      assert.equal(aborts, 1); assert.equal(resolved.length, 0);
      assert.match(first.message, stage === 'readiness' ? /scoped phase proof timed out/ : /phase deadline expired/);
      assert.throws(() => f.phase.web(stage === 'readiness' ? 'READY' : 'REQUEST_SENT'), error => error === first);
      assert.throws(() => f.streams.android.push('stdout', marker(f.scope, 'android', 'INCOMING_VISIBLE')), error => error === first);
      f.advance(900_000); assert.equal(aborts, 1);
    } finally { f.phase.abort(Error('fixture cleanup')); }
  }
});

test('public waiter rejects invalid event, ceilings or receiver and preserves the first explicit abort', {timeout: 2000}, async () => {
  const invalid = [['FOREIGN', 1], ['READY', 0], ['READY', -1], ['READY', 1.5], ['READY', NaN],
    ['READY', Infinity], ['READY', '240000'], ['READY', 240_001], ['REQUEST_SENT', 180_001],
    ['INCOMING_VISIBLE', 180_000], ['INCOMING_VISIBLE', 180_000, 'web'], ['INCOMING_VISIBLE', 180_000, 'foreign']];
  for (const args of invalid) {
    const f = fixture();
    try {
      await assert.rejects(f.phase.waitFor(...args), /Original phase wait ceiling required|Registered receiver required/);
      assert.throws(() => f.phase.releaseSender());
    } finally { f.phase.abort(Error('fixture cleanup')); }
  }
  const f = fixture(), first = Error('SYNTHETIC_FIRST_ABORT');
  const ready = assert.rejects(f.phase.waitFor('READY', 240_000), error => error === first);
  const sent = assert.rejects(f.phase.waitFor('REQUEST_SENT', 180_000), error => error === first);
  f.phase.abort(first); await Promise.all([ready, sent]);
  f.phase.abort(Error('SYNTHETIC_LATER_ABORT'));
  await assert.rejects(f.phase.waitFor('READY', 240_000), error => error === first);
  assert.equal(f.phase.snapshot(query(f.scope, 'android', 'request-sent')).status, 410);
  assert.throws(() => f.phase.web('READY'), error => error === first);
});
