import test from 'node:test';
import assert from 'node:assert/strict';
import { buildFifoContext, matchFifoHead, validateFifoTerminal, createFifoPublication, nativeHeadChoices } from './recovery-fifo.mjs';

const identities = { web: { deviceId: 'web-id', deviceName: 'Chrome on Mac' }, android: { deviceId: 'android-id', deviceName: 'Pixel canonical' }, ios: { deviceId: 'ios-id', deviceName: 'iPhone18,3' } };
const passes = { A: 'pass-a', B: 'pass-b' };
const operation = (requestId, requesterId, secretId) => ({ kind: 'recover', requestId, requesterId, secretId, clientStatus: 'needApprove', outcome: undefined, eligiblePeerIds: Object.values(identities).map((x) => x.deviceId).filter((id) => id !== requesterId), resolutionSource: 'unknown' });
const ops = [operation('claim-a', 'web-id', 'pass-a'), operation('claim-b', 'android-id', 'pass-b')];
const cycle = { number: 10, senderSecrets: { web: 'A', android: 'B' }, approvals: [{ platform: 'ios', secret: 'A', action: 'approve' }, { platform: 'ios', secret: 'B', action: 'approve' }] };
const context = (changes = {}) => buildFifoContext({ runId: 'owned-block', testNumber: 6, cycle, identities, passes, baselineRequestIds: ['old-claim'], projection: { operations: ops }, ...changes });
const head = (requestId = 'claim-b', changes = {}) => ({ kind: 'recover', requestId, requester: requestId === 'claim-a' ? identities.web.deviceName : identities.android.deviceName, subject: requestId === 'claim-a' ? 'A' : 'B', count: 2, ...changes });
const select = (ctx, visible, changes = {}) => matchFifoHead({ context: ctx, head: visible, platform: 'ios', remainingSteps: [1, 2], settledRequestIds: [], bindings: {}, ...changes });

test('B visible first keeps original logical step2 and leaves both requests outstanding before any allow', () => {
  const ctx = context(); const selected = select(ctx, head());
  assert.equal(selected.logicalStep, 2); assert.equal(selected.requestId, 'claim-b'); assert.equal(selected.passId, 'pass-b'); assert.equal(selected.expectedCount, 2);
  assert.deepEqual(ctx.requests.map((x) => x.requestId), ['claim-a', 'claim-b']);
  assert.equal(ctx.approvals[0].logicalStep, 1); assert.equal(ctx.approvals[0].subject, 'A');
  assert(Object.isFrozen(ctx) && Object.isFrozen(ctx.requests) && Object.isFrozen(ctx.identities.ios));
});

test('missing second canonical submit, terminal receipt or old-cycle claim cannot publish a decision context', () => {
  for (const operations of [[ops[0]], [ops[0], { ...ops[1], outcome: 'accepted' }], [ops[0], { ...ops[1], requestId: 'old-claim' }], [...ops, operation('duplicate-b', 'android-id', 'pass-b')]]) {
    assert.throws(() => context({ projection: { operations } }), /submission|pending|unique|cycle/i);
  }
  const publication = createFifoPublication({ runId: 'owned-block', testNumber: 6 });
  publication.publishIdentities(identities); assert.equal(publication.read(10), null);
  assert.throws(() => publication.require(10), /not published/);
});

test('same label with foreign UUID, requester, subject or false per-secret count cannot select a head', () => {
  const ctx = context();
  for (const invalid of [head('unknown-id'), head('claim-b', { requester: 'Android' }), head('claim-b', { subject: 'A' }), head('claim-b', { count: 1 }), head('claim-b', { kind: 'delete-secret' })]) assert.throws(() => select(ctx, invalid));
  assert.throws(() => select(ctx, head(), { platform: 'android' }), /addressed|eligible/);
  assert.throws(() => select(ctx, head(), { settledRequestIds: ['claim-b'] }), /settled/);
});

test('2→1→0 consumes only the terminal UUID and never emits visibility for an unseen second head', () => {
  const ctx = context(); const first = select(ctx, head());
  const acceptedB = { ...ops[1], outcome: 'accepted', resolutionSource: 'verified', clientStatus: 'accepted' };
  validateFifoTerminal({ context: ctx, requestId: first.requestId, outcome: 'accepted', settledRequestIds: [], projection: { operations: [ops[0], acceptedB] } });
  assert.throws(() => validateFifoTerminal({ context: ctx, requestId: first.requestId, outcome: 'accepted', settledRequestIds: [], projection: { operations: ops } }), /terminal/);
  const emitted = [first.logicalStep]; assert.deepEqual(emitted, [2]);
  const second = select(ctx, head('claim-a', { count: 1 }), { remainingSteps: [1], settledRequestIds: ['claim-b'], bindings: { 2: 'claim-b' } });
  emitted.push(second.logicalStep); assert.deepEqual(emitted, [2, 1]);
  validateFifoTerminal({ context: ctx, requestId: second.requestId, outcome: 'accepted', settledRequestIds: ['claim-b'], projection: { operations: [{ ...ops[0], outcome: 'accepted', resolutionSource: 'verified', clientStatus: 'done' }, acceptedB] } });
  assert.throws(() => select(ctx, head('claim-a', { count: 0 }), { remainingSteps: [], settledRequestIds: ['claim-a', 'claim-b'] }));
  assert.throws(() => validateFifoTerminal({ context: ctx, requestId: 'claim-a', outcome: 'accepted', settledRequestIds: [], projection: { operations: [{ ...ops[0], outcome: 'accepted', resolutionSource: 'verified' }, { ...ops[1], outcome: 'accepted', resolutionSource: 'verified' }] } }), /other|pending/);
});

test('same Secret keeps two real sender-owned IDs without inventing a sender-to-step order', () => {
  const sameCycle = { ...cycle, senderSecrets: { web: 'A', android: 'A' }, approvals: [{ platform: 'ios', secret: 'A', action: 'approve' }, { platform: 'ios', secret: 'A', action: 'approve' }] };
  const ctx = context({ cycle: sameCycle, projection: { operations: [ops[0], operation('claim-other-a', 'android-id', 'pass-a')] } });
  const first = select(ctx, head('claim-other-a', { subject: 'A' })); assert.equal(first.logicalStep, 1); assert.equal(first.requestId, 'claim-other-a');
  const second = select(ctx, head('claim-a', { count: 1 }), { settledRequestIds: ['claim-other-a'], remainingSteps: [2], bindings: { 1: 'claim-other-a' } });
  assert.equal(second.logicalStep, 2); assert.equal(second.requestId, 'claim-a');
  assert.throws(() => select(ctx, head('claim-other-a', { subject: 'A' }), { bindings: { 1: 'claim-other-a' }, remainingSteps: [2] }));
});

test('split peers retain original responder assignment and sender cannot approve its own request', () => {
  const split = context({ cycle: { ...cycle, approvals: [{ platform: 'web', secret: 'B' }, { platform: 'android', secret: 'A' }] } });
  const web = matchFifoHead({ context: split, platform: 'web', head: head('claim-b', { count: 1 }), remainingSteps: [1, 2], settledRequestIds: [], bindings: {} });
  assert.equal(web.logicalStep, 1); assert.equal(web.requesterId, 'android-id');
  const android = matchFifoHead({ context: split, platform: 'android', head: head('claim-a', { count: 1 }), remainingSteps: [1, 2], settledRequestIds: [], bindings: {} });
  assert.equal(android.logicalStep, 2); assert.throws(() => matchFifoHead({ context: split, platform: 'web', head: head('claim-a', { count: 1 }), remainingSteps: [1, 2], settledRequestIds: [], bindings: {} }));
});

test('publication is immutable/run-bound and exact native choice includes Claim UUID plus original step', () => {
  const ctx = context(); const publication = createFifoPublication({ runId: 'owned-block', testNumber: 6 });
  publication.publishIdentities(identities); assert.throws(() => publication.publish({ ...ctx, cycle: 11 }), /immutable/); publication.publish(ctx); assert.equal(publication.require(10), ctx);
  assert.throws(() => publication.publish({ ...ctx, runId: 'foreign-run' })); assert.throws(() => publication.publish({ ...ctx, blockId: 'other-block' })); assert.throws(() => publication.publish(ctx), /already/);
  assert.throws(() => publication.publishIdentities({ ...identities, web: { ...identities.web, deviceId: 'foreign' } }), /immutable/);
  const choices = nativeHeadChoices(ctx, 'ios', [1, 2], []);
  assert.equal(choices.length, 2); assert(choices.some((x) => x.marker === 'E2E: IOS_INCOMING_VISIBLE_10_2 claimId=claim-b'));
  assert.equal(nativeHeadChoices(ctx, 'ios', [1], ['claim-b']).length, 1);
});

test('requester navigation metadata remains unavailable until the SAME two canonical pending submissions exist', () => {
  const splitCycle = { ...cycle, approvals: [{ platform: 'web', secret: 'B' }, { platform: 'android', secret: 'A' }] };
  const publication = createFifoPublication({ runId: 'owned-block', testNumber: 6 });
  publication.publishIdentities(identities);
  for (const operations of [[ops[0]], [ops[1]], [ops[0], { ...ops[1], outcome: 'accepted' }]]) {
    assert.throws(() => context({ cycle: splitCycle, projection: { operations } }), /submission|pending/i);
    assert.equal(publication.read(10), null);
  }
  const submitted = context({ cycle: splitCycle }); publication.publish(submitted);
  assert.equal(publication.require(10), submitted);
  assert.equal(matchFifoHead({ context: submitted, platform: 'android', head: head('claim-a', { count: 1 }),
    remainingSteps: [1, 2], settledRequestIds: [], bindings: {} }).requestId, 'claim-a');
  assert.equal(publication.require(10), submitted);
});

test('cross-source protocol guard: native requester-approvers await canonical context before pending navigation and reuse it', async () => {
  // Source-contract regression only: this does not execute Kotlin/Swift or diagnose an actual native phase.
  const { readFileSync } = await import('node:fs');
  const android = readFileSync(new URL('../../../meta-secret-compose/composeApp/src/androidTest/kotlin/metasecret/project/com/CaseFourAndroidConcurrentRecoveryTest.kt', import.meta.url), 'utf8');
  const ios = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseFourIosConcurrentRecoveryUITest.swift', import.meta.url), 'utf8');
  const a = android.slice(android.indexOf('for (cycle in recoveryCyclePlan()) {'), android.indexOf('val consumed = mutableSetOf<Int>()'));
  const s = ios.slice(ios.indexOf('for cycle in recoveryPlan {'), ios.indexOf('var remaining = originalSteps'));
  for (const [source, sent, awaited, navigation, reuse] of [
    [a, 'marker("ANDROID_RECOVERY_REQUEST_SENT_${cycle.number}")', 'submittedContext = lbFifoContext(', 'closeShowSecretDialog()', 'submittedContext ?: lbFifoContext('],
    [s, 'print("E2E: IOS_RECOVERY_REQUEST_SENT_\\(cycle.number)")', 'let context = app.lbLoadFifo(', 'closeShowSecretDialog()', 'submittedContext ?? app.lbLoadFifo('],
  ]) {
    const positions = [sent, awaited, navigation, reuse].map(value => source.indexOf(value));
    assert(positions.every(position => position >= 0));
    assert(positions.every((position, index) => index === 0 || position > positions[index - 1]));
  }
  assert.match(a, /if \(ownSecret != null\)[\s\S]*if \(cycle\.approvals\.any \{ it\.platform == "android" \}\) \{\s*if \(cycle\.senderSecrets\.size == 2\)/);
  assert.match(s, /if let ownSecret = cycle\.senderSecrets\["ios"\][\s\S]*if cycle\.approvals\.contains\(where: \{ \$0\.platform == "ios" \}\) \{\s*if cycle\.senderSecrets\.count == 2/);
  assert.match(s, /XCTFail\("Concurrent recovery requires the startup run-scoped FIFO context"\); return/);
});
