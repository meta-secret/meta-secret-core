import test from 'node:test';
import assert from 'node:assert/strict';
import { recoveryClaimIds, selectNewPendingRecoveryClaim } from './test15-recovery-claims.mjs';

const oldWebClaim = {
  id: 'old-web-claim',
  type: 'Recover',
  password: 'test-secret',
  sender: 'web-device',
  receivers: [{ id: 'cli-device', status: 'Pending' }],
};
const currentAndroidClaim = {
  id: 'current-android-claim',
  type: 'Recover',
  password: 'test-secret',
  sender: 'android-device',
  receivers: [{ id: 'cli-device', status: 'Pending' }],
};

test('Test15 CLI approver ignores an older pending recovery and selects the new sender claim', () => {
  const baseline = recoveryClaimIds({ claims: [oldWebClaim] }, 'test-secret');
  const selected = selectNewPendingRecoveryClaim(
    { claims: [oldWebClaim, currentAndroidClaim] },
    { password: 'test-secret', baselineClaimIds: baseline, receiverId: 'cli-device' },
  );
  assert.equal(selected, currentAndroidClaim);
});

test('Test15 CLI recovery selection can require the configured sender', () => {
  const baseline = recoveryClaimIds({ claims: [oldWebClaim] }, 'test-secret');
  assert.equal(selectNewPendingRecoveryClaim(
    { claims: [oldWebClaim, currentAndroidClaim] },
    {
      password: 'test-secret',
      baselineClaimIds: baseline,
      receiverId: 'cli-device',
      senderId: 'android-device',
    },
  ), currentAndroidClaim);
  assert.equal(selectNewPendingRecoveryClaim(
    { claims: [oldWebClaim, currentAndroidClaim] },
    {
      password: 'test-secret',
      baselineClaimIds: baseline,
      receiverId: 'cli-device',
      senderId: 'ios-device',
    },
  ), null);
});


// Capabilities are checked inside each case: the old module must still collect its CLI controls.
import * as fifo from './test15-recovery-claims.mjs';
import vm from 'node:vm';
const capability = (name) => { assert.equal(typeof fifo[name], 'function', `Test15 FIFO capability absent: ${name}`); return fifo[name]; };
const clone = (value) => structuredClone(value);
const R = '11111111-1111-4111-8111-111111111111', M = '22222222-2222-4222-8222-222222222222';
const scope = { runId: 'effective-run-15', testNumber: 15, block: 'scenario-block1', cycle: 1, step: 1 };
const planInput = () => ({ scope: clone(scope), vaultId: 'same-vault', sender: 'web', removalApprover: 'android', removalTarget: 'ios',
  approvals: [{ platform: 'android', secret: 'test-secret', action: 'decline' }],
  secret: { passId: 'exact-pass', name: 'test-secret' }, baselineRequestIds: ['old-recover'],
  devices: { web: { id: 'A', name: 'Web A' }, android: { id: 'B', name: 'Android B' }, ios: { id: 'C', name: 'iPhone18,3' } } });
const context = () => capability('test15FifoContext')(planInput());
function projection(phase = 'both-pending') {
  const recover = { kind: 'recover', requestId: R, requesterId: 'A', secretId: 'exact-pass', clientStatus: 'pending',
    eligiblePeerIds: ['B', 'C'], canApprove: false, canDecline: false, resolutionSource: 'unknown', readiness: 'waiting' };
  const remove = { kind: 'removeDevice', requestId: M, requesterId: 'A', targetDeviceId: 'C', clientStatus: 'pending',
    eligiblePeerIds: ['B'], canApprove: false, canDecline: false, resolutionSource: 'unknown', readiness: 'waiting' };
  if (['recover-declined', 'removed'].includes(phase)) Object.assign(recover, {
    clientStatus: 'declined', outcome: 'declined', responderId: 'B', resolutionSource: 'verified', eligiblePeerIds: [] });
  if (phase === 'removed') Object.assign(remove, {
    clientStatus: 'accepted', outcome: 'accepted', responderId: 'B', resolutionSource: 'canonical', eligiblePeerIds: [], readiness: 'done' });
  return { session: { identity: 'owned-current', member: true, ready: true, vaultId: 'same-vault' },
    devices: Object.entries(planInput().devices).filter(([role]) => phase !== 'removed' || role !== 'ios')
      .map(([role, device]) => ({ deviceId: device.id, name: device.name, current: role === 'web' })),
    secrets: [{ passId: 'exact-pass', name: 'test-secret' }], operations: phase === 'recover-pending' ? [recover] : [recover, remove] };
}
const input = (phase, extra = {}) => ({ context: context(), phase, recoveryId: R, removalId: M, ...extra });
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const turn = async () => { for (let i = 0; i < 8; i++) await Promise.resolve(); };
const prefixes = { ready: 'ANDROID_TEST15_REMOVAL_APPROVER_READY', 'recovery-head': 'ANDROID_TEST15_RECOVERY_HEAD_VISIBLE_1_1',
  'recovery-declined': 'ANDROID_TEST15_RECOVERY_DECLINED_1_1', 'removal-head': 'ANDROID_TEST15_REMOVAL_REQUEST_VISIBLE',
  'removal-approved': 'ANDROID_TEST15_REMOVAL_APPROVED_RECOVERY_CLEARED' };
function marker(stage, patch = {}) {
  const claimId = stage.startsWith('recovery') ? R : stage.startsWith('removal') ? M : undefined;
  const count = { ready: 0, 'recovery-head': 2, 'recovery-declined': 1, 'removal-head': 1, 'removal-approved': 0 }[stage];
  return `E2E: ${prefixes[stage]} ${JSON.stringify({ ...scope, claimId, count, ...patch })}`;
}
function sequenceFixture() {
  const trace = [], gates = [], pending = new Set(), passive = deferred(), actor = deferred();
  const frames = new Map(['both-pending', 'recover-declined', 'removed'].map((phase) => [phase, deferred()]));
  const heads = new Map(['recovery-head', 'removal-head'].map((stage) => [stage, deferred()]));
  let drainHold = Promise.resolve();
  const owned = (promise, signal) => new Promise((resolve, reject) => {
    const ticket = {}; pending.add(ticket);
    const cleanup = () => { pending.delete(ticket); signal.removeEventListener('abort', abort); };
    const abort = () => { cleanup(); reject(signal.reason ?? new Error('owned wait cancelled')); };
    if (signal.aborted) return abort();
    signal.addEventListener('abort', abort, { once: true });
    promise.then((value) => { cleanup(); resolve(value); }, (error) => { cleanup(); reject(error); });
  });
  const ports = {
    passiveResult: passive.promise,
    async startActor(actual) { trace.push('startActor'); assert.deepEqual(actual.scope, scope); assert.equal(actual.removalApprover, 'android'); assert.deepEqual(actual.devices, planInput().devices); return { result: actor.promise }; },
    async requestRemoval(actual) { trace.push('requestRemoval'); assert.equal(actual.devices.ios.name, 'iPhone18,3'); },
    waitMarker(stage, { signal }) { trace.push(`marker:${stage}`); return owned(heads.get(stage)?.promise ?? Promise.resolve(marker(stage)), signal); },
    observe(actual, signal) { trace.push(`observe:${actual.phase}`); return owned(frames.get(actual.phase).promise, signal); },
    allowGate(actual) { trace.push(`gate:${actual.name}`); gates.push(actual); },
    async drain() { trace.push('drain'); await drainHold; assert.equal(pending.size, 0); },
  };
  return { trace, gates, pending, passive, actor, frames, heads, ports, setDrain(value) { drainHold = value; } };
}
function observationFixture() {
  const subscribers = new Set(), timers = new Map(); let reads = 0, hold = null;
  const state = { appManager: { get_state() { throw new Error('forbidden command'); }, refresh() { throw new Error('forbidden refresh'); } },
    sessionGeneration: 1, currState: { projection: projection('recover-pending') }, isSynchronizing: false, requiredSyncPending: false, syncError: '',
    $subscribe(callback) { subscribers.add(callback); return () => subscribers.delete(callback); } };
  const runtime = { state, async readOwnedMember({ state: current }, mapper) { reads++; if (hold) await hold; return mapper(current); },
    readCoreProjection: (member) => clone(member.projection), setTimeout(callback) { const id = Symbol(); timers.set(id, callback); return id; }, clearTimeout(id) { timers.delete(id); } };
  return { state, runtime, subscribers, timers, get reads() { return reads; }, hold(promise) { hold = promise; },
    publish(next, extra = {}) { Object.assign(state, extra); state.currState = { projection: next }; for (const callback of subscribers) callback(); } };
}

test('Test15 explicit FIFO context is immutable and accepts exactly the owner-authorized B Decline', () => {
  const make = capability('test15FifoContext'), source = planInput(), result = make(source);
  assert.deepEqual(result, source); assert(Object.isFrozen(result) && Object.isFrozen(result.scope) && Object.isFrozen(result.devices.ios));
  source.devices.ios.id = 'foreign'; source.approvals[0].action = 'approve';
  assert.equal(result.devices.ios.id, 'C'); assert.equal(result.approvals[0].action, 'decline');
});

test('Test15 rejects role swaps, self-response, extra approvals and mismatched effective scope', () => {
  const make = capability('test15FifoContext');
  const changes = [(p) => { p.approvals = []; }, (p) => { p.approvals.push(clone(p.approvals[0])); },
    (p) => { p.approvals[0].action = 'approve'; }, (p) => { p.approvals[0].platform = 'web'; },
    (p) => { p.approvals[0].secret = 'foreign'; }, (p) => { p.removalTarget = 'android'; },
    (p) => { p.sender = 'android'; }, (p) => { p.scope.runId = ''; }, (p) => { p.scope.testNumber = 4; },
    (p) => { p.scope.block = 'scenario-block9'; }, (p) => { p.scope.cycle = 2; }, (p) => { p.scope.step = 2; }, (p) => { p.devices.android.id = 'A'; }];
  for (const change of changes) { const p = planInput(); change(p); assert.throws(() => make(p), /Test15|scope|approval|role|identity/i); }
});

test('Test15 blocks2 and3 retain their original no-explicit-response branch without input mutation', () => {
  const make = capability('test15FifoContext');
  for (const [block, sender, target] of [['scenario-block2', 'ios', 'android'], ['scenario-block3', 'android', 'ios']]) {
    const p = planInput(); Object.assign(p.scope, { block }); Object.assign(p, { sender, removalApprover: sender, removalTarget: target, approvals: [] });
    const before = clone(p); assert.equal(make(p), null); assert.deepEqual(p, before);
  }
});

test('Test15 binds one genuinely new pending R to exact A Pass and addressed B/C, rejecting stale or foreign submissions', () => {
  const guard = capability('assertTest15FifoTransition'), request = input('recover-pending', { recoveryId: undefined, removalId: undefined });
  assert.deepEqual(guard(request, projection('recover-pending')), { recoveryId: R });
  const empty = projection('recover-pending'); empty.operations = []; assert.equal(guard(request, empty), null);
  const changes = [(p) => { p.operations[0].requestId = 'old-recover'; }, (p) => { p.operations.push(clone(p.operations[0])); },
    (p) => { p.operations[0].requesterId = 'B'; }, (p) => { p.operations[0].secretId = 'foreign'; },
    (p) => { p.operations[0].eligiblePeerIds = ['A', 'C']; }, (p) => { p.operations[0].outcome = 'declined'; },
    (p) => { p.devices[2].name = 'iPhone18'; }, (p) => { p.secrets[0].name = 'foreign'; }, (p) => { p.session.vaultId = 'foreign-vault'; }];
  for (const change of changes) { const p = projection('recover-pending'); change(p); assert.throws(() => guard(request, p), /Test15|recovery|identity|pending|unique|member/i); }
});

test('Test15 pending M is exact A-to-C and does not invalidate R or three-member membership', () => {
  const guard = capability('assertTest15FifoTransition'), request = input('both-pending', { removalId: undefined });
  assert.deepEqual(guard(request, projection()), { recoveryId: R, removalId: M });
  for (const change of [(p) => p.devices.pop(), (p) => { p.operations[1].targetDeviceId = 'B'; },
    (p) => { p.operations[1].requesterId = 'B'; }, (p) => { p.operations[0].requestId = M; },
    (p) => { p.operations[0].outcome = 'declined'; }, (p) => { p.operations.push({ ...p.operations[1], requestId: 'foreign-M' }); }]) {
    const p = projection(); change(p); assert.throws(() => guard(request, p), /Test15|pending|identity|member|unique|claim/i);
  }
});

test('Test15 canonical B decline must precede Remove approval and persist through exact A/B membership', () => {
  const guard = capability('assertTest15FifoTransition');
  assert.deepEqual(guard(input('recover-declined'), projection('recover-declined')), { recoveryId: R, removalId: M });
  assert.deepEqual(guard(input('removed'), projection('removed')), { recoveryId: R, removalId: M });
  for (const [phase, change] of [['recover-declined', (p) => { p.operations[0].responderId = 'A'; }],
    ['recover-declined', (p) => { p.operations[0].resolutionSource = 'unknown'; }],
    ['recover-declined', (p) => { p.operations[1].outcome = 'accepted'; }], ['recover-declined', (p) => { p.operations[1].requestId = 'foreign-M'; }], ['recover-declined', (p) => p.devices.pop()],
    ['removed', (p) => { p.operations[1].requestId = R; }], ['removed', (p) => { p.operations[1].responderId = 'C'; }],
    ['removed', (p) => { p.operations[0].outcome = 'accepted'; }], ['removed', (p) => { p.devices[1].deviceId = 'C'; }]]) {
    const p = projection(phase); change(p); assert.throws(() => guard(input(phase), p), /Test15|canonical|declin|identity|member|claim|responder/i);
  }
});

test('Test15 awaits passive actor, actual R/count2 and canonical B decline before exact M/count1 gate', { timeout: 2_000 }, async () => {
  const run = capability('runTest15FifoDecisions'), f = sequenceFixture(), ctx = context();
  const task = run({ context: ctx, recoveryId: R }, f.ports); await turn(); assert.deepEqual(f.trace, []);
  f.passive.resolve('authoritative receiver passed'); await turn(); assert.deepEqual(f.trace, ['startActor', 'marker:ready', 'requestRemoval', 'observe:both-pending']);
  assert.equal(f.gates.length, 0); f.frames.get('both-pending').resolve(projection()); await turn(); assert.equal(f.gates.length, 0);
  f.heads.get('recovery-head').resolve(marker('recovery-head')); await turn(); assert.deepEqual(f.gates.map((g) => g.name), ['test15-recover-decline']);
  assert.equal(f.trace.at(-1), 'observe:recover-declined'); f.frames.get('recover-declined').resolve(projection('recover-declined')); await turn();
  assert.equal(f.gates.length, 1); f.heads.get('removal-head').resolve(marker('removal-head')); await turn();
  assert.deepEqual(f.gates, [{ name: 'test15-recover-decline', cycle: 1, requestId: R, scope }, { name: 'test15-remove-approve', cycle: 1, requestId: M, scope }]);
  let done = false; void task.then(() => { done = true; }); await turn(); assert.equal(done, false);
  f.frames.get('removed').resolve(projection('removed')); assert.deepEqual(await task, { recoveryId: R, removalId: M });
  assert.equal(f.trace.at(-1), 'drain'); assert.equal(f.pending.size, 0); assert.equal(f.gates.length, 2);
});

test('Test15 rejects early Remove, wrong run/cycle/step, wrong UUID/count and non-complete matched head lines', { timeout: 2_000 }, async () => {
  const run = capability('runTest15FifoDecisions');
  const badLines = [marker('removal-head'), marker('recovery-head', { runId: 'other-run' }), marker('recovery-head', { cycle: 2 }),
    marker('recovery-head', { step: 2 }), marker('recovery-head', { claimId: M }), marker('recovery-head', { count: 1 }), marker('recovery-head') + ' appended'];
  for (const bad of badLines) {
    const f = sequenceFixture(), task = run({ context: context(), recoveryId: R }, f.ports); const failed = assert.rejects(task, /Test15|head|marker|scope|count/i);
    f.passive.resolve('passed'); f.frames.get('both-pending').resolve(projection()); f.heads.get('recovery-head').resolve(bad);
    await failed; assert.equal(f.gates.length, 0); assert.equal(f.trace.at(-1), 'drain'); assert.equal(f.pending.size, 0);
  }
});

test('Test15 exact next M count1 and terminal count0 are required after the B-decline barrier', { timeout: 2_000 }, async () => {
  const run = capability('runTest15FifoDecisions');
  for (const stage of ['removal-head', 'removal-approved']) {
    const f = sequenceFixture();
    if (stage === 'removal-approved') {
      const original = f.ports.waitMarker;
      f.ports.waitMarker = (name, options) => name === stage ? Promise.resolve(marker(stage, { count: 1 })) : original(name, options);
    }
    const task = run({ context: context(), recoveryId: R }, f.ports), failed = assert.rejects(task, /Test15|count|marker|head/i);
    f.passive.resolve('passed'); f.frames.get('both-pending').resolve(projection()); f.heads.get('recovery-head').resolve(marker('recovery-head'));
    f.frames.get('recover-declined').resolve(projection('recover-declined'));
    f.heads.get('removal-head').resolve(marker('removal-head', { count: stage === 'removal-head' ? 2 : 1 }));
    await failed; assert.equal(f.gates.length, stage === 'removal-head' ? 1 : 2); assert.equal(f.pending.size, 0); assert.equal(f.trace.at(-1), 'drain');
  }
});

test('Test15 a marker is insufficient: contradictory canonical responder or M target cannot open Remove gate', { timeout: 2_000 }, async () => {
  const run = capability('runTest15FifoDecisions');
  for (const change of [(p) => { p.operations[0].responderId = 'A'; }, (p) => { p.operations[1].targetDeviceId = 'B'; }]) {
    const f = sequenceFixture(), task = run({ context: context(), recoveryId: R }, f.ports); const failed = assert.rejects(task, /Test15|canonical|identity|responder|target/i);
    f.passive.resolve('passed'); f.frames.get('both-pending').resolve(projection()); f.heads.get('recovery-head').resolve(marker('recovery-head'));
    const p = projection('recover-declined'); change(p); f.frames.get('recover-declined').resolve(p);
    await failed; assert.equal(f.gates.length, 1); assert.equal(f.gates[0].requestId, R); assert.equal(f.pending.size, 0);
  }
});

test('Test15 failed passive receiver prevents a second Android actor from starting', { timeout: 2_000 }, async () => {
  const run = capability('runTest15FifoDecisions'), f = sequenceFixture(), cause = new Error('actual passive actor failed');
  const task = run({ context: context(), recoveryId: R }, f.ports); const failed = assert.rejects(task, (error) => error === cause);
  f.passive.reject(cause); await failed; assert.equal(f.trace.includes('startActor'), false); assert.equal(f.gates.length, 0);
});

test('Test15 primary actor failure cancels owned observer and awaits drain without later gates', { timeout: 2_000 }, async () => {
  const run = capability('runTest15FifoDecisions'), f = sequenceFixture(), drained = deferred(), cause = new Error('actual Case37 actor failed');
  f.setDrain(drained.promise); const task = run({ context: context(), recoveryId: R }, f.ports); let finished = false;
  const failed = assert.rejects(task, (error) => error === cause).then(() => { finished = true; });
  f.passive.resolve('passed'); await turn(); assert(f.trace.includes('observe:both-pending'));
  f.actor.reject(cause); await turn(); assert(f.trace.includes('drain')); assert.equal(finished, false); assert.equal(f.pending.size, 0);
  drained.resolve(); await failed; assert.equal(f.gates.length, 0);
});

test('Test15 read-only observer waits for genuine current ready publication and removes owned listeners', { timeout: 2_000 }, async () => {
  const observe = capability('waitForTest15FifoSnapshot'), f = observationFixture(); f.state.requiredSyncPending = true;
  const wait = observe(input('both-pending'), f.runtime); await turn(); assert.equal(f.reads, 0); assert.equal(f.subscribers.size, 1);
  f.publish(projection(), { requiredSyncPending: false }); const result = await wait;
  assert.equal(result.operations[1].requestId, M); assert.equal(f.subscribers.size, 0); assert.equal(f.timers.size, 0);
});

test('Test15 retired manager/generation or failed sync cannot admit a late owned read', { timeout: 2_000 }, async () => {
  const observe = capability('waitForTest15FifoSnapshot');
  for (const extra of [{ appManager: {} }, { sessionGeneration: 2 }, { syncError: 'actual current sync failed' }]) {
    const f = observationFixture(), read = deferred(); f.hold(read.promise);
    const wait = observe(input('recover-pending'), f.runtime); const failed = assert.rejects(wait, /session|owner|retir|synchronization|sync/i);
    await turn(); f.publish(projection('recover-pending'), extra); read.resolve(); await failed;
    assert.equal(f.reads, 1); assert.equal(f.subscribers.size, 0); assert.equal(f.timers.size, 0);
  }
});

test('Test15 obsolete frame cannot publish, and timeout or actor abort drains the observer', { timeout: 2_000 }, async () => {
  const observe = capability('waitForTest15FifoSnapshot'), f = observationFixture(), read = deferred(); f.hold(read.promise);
  const wait = observe(input('both-pending'), f.runtime); await turn(); f.hold(null); f.publish(projection()); read.resolve();
  const actual = await wait; assert.equal(actual.operations[1].requestId, M); assert(f.reads >= 2); assert.equal(f.subscribers.size, 0);
  const g = observationFixture(), timeout = observe(input('both-pending', { timeoutMs: 10 }), g.runtime);
  const timedOut = assert.rejects(timeout, /Timed out|deadline/i); await turn(); [...g.timers.values()][0](); await timedOut;
  assert.equal(g.subscribers.size, 0); assert.equal(g.timers.size, 0);
  const h = observationFixture(), controller = new AbortController();
  const aborted = observe(input('both-pending', { signal: controller.signal }), h.runtime); const cancelled = assert.rejects(aborted, /cancelled/i);
  await turn(); controller.abort(new Error('actor cancelled')); await cancelled; assert.equal(h.subscribers.size, 0); assert.equal(h.timers.size, 0);
});


test('Test15 serialized actual browser observer works without module closures and cleans its owned read', { timeout: 2_000 }, async () => {
  const actual = capability('waitForTest15FifoSnapshot'), f = observationFixture();
  const serialized = vm.runInNewContext(`(${actual.toString()})`, Object.create(null), { timeout: 100 });
  f.state.requiredSyncPending = true;
  const wait = serialized(input('both-pending'), f.runtime); await turn();
  assert.equal(f.reads, 0); assert.equal(f.subscribers.size, 1);
  f.publish(projection(), { requiredSyncPending: false }); const snapshot = await wait;
  assert.equal(snapshot.operations[0].requestId, R); assert.equal(snapshot.operations[1].requestId, M);
  assert(f.reads >= 1); assert.equal(f.subscribers.size, 0); assert.equal(f.timers.size, 0);
});
