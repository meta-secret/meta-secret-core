// Ordinary host regression tests import the actual shared test-harness module.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {createTest24RecoveryPhase} from './recovery-phase.mjs';
const platforms = ['web', 'android', 'ios'];
const contextId = 'a'.repeat(32);
const invocation = {web: '1'.repeat(32), android: '2'.repeat(32), ios: '3'.repeat(32)};
function fixture(sender = 'ios') {
  let now = 0;
  const owners = Object.fromEntries(platforms.map(p => [p, {}]));
  const binding = {version: 1, runId: 'SYNTHETIC_UNIT_RUN', testNumber: 24, attempt: 1,
    blockId: 'SYNTHETIC_UNIT_BLOCK', cycle: 13, logicalStep: 1,
    mode: 'ios-multi-secret-lifecycle', contextId, sender,
    participants: platforms.map(platform => ({platform, role: platform === sender ? 'sender' : 'receiver',
      invocationId: invocation[platform]}))};
  const phase = createTest24RecoveryPhase({binding, now: () => now});
  const streams = Object.fromEntries(platforms.filter(p => p !== 'web').map(p => [p, phase.observer(p, owners[p])]));
  return {phase, binding, owners, streams, time: value => {now = value;}};
}
function line(platform, event, overrides = {}) {
  const fields = {context: contextId, invocation: invocation[platform], platform, phase: event, ...overrides};
  const text = `E2E: RECOVERY_PHASE_V1 context=${fields.context} invocation=${fields.invocation} platform=${fields.platform} phase=${fields.phase}`;
  return platform === 'android' ? `11-03 00:15:35.985 18342 18362 I MetaSecretE2E: ${text}\n` : text + '\n';
}
function event(f, platform, value) {
  if (platform === 'web') f.phase.web(value);
  else f.streams[platform].push('stdout', line(platform, value));
}
function ready(f) { for (const p of platforms) event(f, p, 'READY'); }
function query(platform, view = platform === 'ios' ? 'dispatch' : 'request-sent') {
  return `/recovery-phase?v=1&context=${contextId}&participant=${invocation[platform]}&view=${view}`;
}

test('all real READY before dispatch, actual SENT before both receiver badges and approval', () => {
  const f = fixture();
  assert.equal(f.phase.snapshot(query('ios')).body.state, 'WAITING');
  assert.throws(() => f.phase.releaseSender());
  ready(f); f.time(146_484); f.phase.releaseSender();
  assert.equal(f.phase.snapshot(query('ios')).body.state, 'DISPATCH_PERMITTED');
  assert.equal(f.phase.snapshot(query('android')).body.state, 'WAITING');
  f.time(199_362); event(f, 'ios', 'REQUEST_SENT');
  assert.equal(f.phase.sentAt, 199_362);
  assert.equal(f.phase.badgeDeadlineAt, 379_362);
  f.time(200_351); event(f, 'web', 'INCOMING_VISIBLE');
  assert.throws(() => f.phase.requireBothBadges());
  event(f, 'android', 'INCOMING_VISIBLE');
  assert.doesNotThrow(() => f.phase.requireBothBadges());
  assert.equal(f.phase.snapshot(query('android')).body.badgeRemainingMs, 179_011);
});

test('all three original sender roles use same scope, no platform-specific omission', () => {
  for (const sender of platforms) {
    const f = fixture(sender); ready(f); f.phase.releaseSender();
    f.time(10); event(f, sender, 'REQUEST_SENT');
    for (const receiver of platforms.filter(p => p !== sender)) event(f, receiver, 'INCOMING_VISIBLE');
    assert.doesNotThrow(() => f.phase.requireBothBadges());
  }
});

test('one stricter600 origin includes startup, actual iOS receipt only shortens, no SENT reset', () => {
  const f = fixture(); assert.equal(f.phase.deadlineAt, 600_000);
  f.time(100_000); f.phase.setIosDeadline(650_000); assert.equal(f.phase.deadlineAt, 600_000);
  f.phase.setIosDeadline(550_000); assert.equal(f.phase.deadlineAt, 550_000);
  f.time(400_000); ready(f); f.phase.releaseSender(); f.time(500_000); event(f, 'ios', 'REQUEST_SENT');
  assert.equal(f.phase.snapshot(query('android')).body.deadlineRemainingMs, 50_000);
  assert.equal(f.phase.snapshot(query('android')).body.badgeRemainingMs, 50_000);
  f.time(550_000); assert.throws(() => f.phase.assertActive());
  assert.equal(f.phase.snapshot(query('android')).status, 408);
});

test('original sender180 deadline rejects the old400s-after-release fixture', () => {
  const f = fixture(); f.time(100_000); ready(f); f.phase.releaseSender();
  f.time(500_000); assert.throws(() => event(f, 'ios', 'REQUEST_SENT'));
  assert.equal(f.phase.snapshot(query('android')).status, 408);
});

test('SENT fixed once; badge deadline equality expires even if overall remains', () => {
  const f = fixture(); ready(f); f.phase.releaseSender(); f.time(1); event(f, 'ios', 'REQUEST_SENT');
  f.time(180_001); assert.equal(f.phase.snapshot(query('android')).status, 408);
  assert.throws(() => event(f, 'android', 'INCOMING_VISIBLE'));
});

test('scope and actual owned live stream cannot be replaced by wrong source or replay', () => {
  const faults = [
    s => s.push('stderr', line('android', 'READY')),
    s => s.push('stdout', line('android', 'READY', {context: 'b'.repeat(32)})),
    s => s.push('stdout', line('android', 'READY', {invocation: 'c'.repeat(32)})),
    s => s.push('stdout', line('android', 'READY', {platform: 'ios'})),
    s => s.push('stdout', '[FAILURE DIAGNOSTIC] ' + line('android', 'READY')),
    s => s.push('stdout', line('android', 'READY').replace(' I MetaSecretE2E:', ' E ForeignTag:')),
    s => s.push('stdout', 'x'.repeat(2049) + line('android', 'READY')),
  ];
  for (const fault of faults) {
    const f = fixture(); assert.throws(() => fault(f.streams.android));
    assert.throws(() => f.phase.releaseSender());
  }
  const f = fixture();
  assert.throws(() => f.phase.observer('ios', f.owners.android));
  const text = line('android', 'READY');
  const g = fixture(); g.streams.android.push('stdout', text.slice(0, 31));
  assert.equal(g.phase.snapshot(query('ios')).body.state, 'WAITING');
  g.streams.android.push('stdout', text.slice(31));
  assert.throws(() => g.streams.android.push('stdout', text)); // no duplicate evidence
});

test('pre-grant SENT, sender badge, pre-SENT badge and duplicate READY cannot progress', () => {
  for (const [platform, phase] of [['ios', 'REQUEST_SENT'], ['ios', 'INCOMING_VISIBLE'], ['android', 'INCOMING_VISIBLE']]) {
    const f = fixture(); assert.throws(() => event(f, platform, phase));
  }
  const f = fixture(); event(f, 'android', 'READY'); assert.throws(() => event(f, 'android', 'READY'));
});

test('duplicate query parameters differ from JSON keys, exact four request keys only', () => {
  const invalid = [query('ios') + '&v=1', query('ios') + '&participant=' + invocation.ios,
    query('ios') + '&extra=1', query('ios').replace('v=1', 'v=1.0'),
    query('ios').replace(contextId, 'f'.repeat(32)), query('ios', 'request-sent'),
    query('android', 'dispatch'), query('android').replace(invocation.android, 'f'.repeat(32))];
  for (const value of invalid) {
    const f = fixture(); assert.ok([400, 404].includes(f.phase.snapshot(value).status));
  }
});

test('abort terminal: no new grants/proofs, no late buffered completion authority', () => {
  const f = fixture(); ready(f); f.phase.releaseSender(); f.phase.abort();
  assert.equal(f.phase.snapshot(query('ios')).status, 410);
  assert.equal(f.phase.snapshot(query('android')).status, 410);
  assert.throws(() => f.phase.releaseSender());
  assert.throws(() => event(f, 'ios', 'REQUEST_SENT'));
  assert.throws(() => f.phase.requireBothBadges());
  // Deliberately no assertion that an already admitted remote UI input was undone.
});

test('binding input validation and immutable full scope, exact Test24 opt-in only', () => {
  for (const change of [{testNumber: 33}, {attempt: true}, {cycle: 12}, {cycle: 40},
    {logicalStep: 2}, {runId: ''}, {blockId: ''}, {mode: 'other'}, {contextId: 'not-a-context'}]) {
    const f = fixture(); assert.throws(() => createTest24RecoveryPhase({binding: {...f.binding, ...change}, now: () => 0}));
  }
  const f = fixture(); f.binding.runId = 'MUTATED'; f.binding.participants[0].role = 'sender';
  assert.equal(f.phase.contextFor('android').runId, 'SYNTHETIC_UNIT_RUN');
  assert.equal(f.phase.contextFor('android').role, 'receiver');
});
