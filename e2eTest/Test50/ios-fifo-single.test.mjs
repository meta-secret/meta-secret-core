import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runSingleFifoFixture } from './ios-fifo-single-fixture.mjs';

let results;
function actualResults() {
  if (results) return results;
  const root = '../../../meta-secret-compose/iosApp/iosAppUITests/';
  const read = name => readFileSync(new URL(root + name, import.meta.url), 'utf8');
  results = runSingleFifoFixture(read('LiveboardInboxTestActions.swift'), read('CaseFourIosConcurrentRecoveryUITest.swift'), read('LiveboardIncomingObservation.swift'));
  return results;
}
const requestIds = ['11111111-1111-4111-8111-111111111111', '22222222-2222-4222-8222-222222222222'];
const marker = (step, id) => `E2E: IOS_INCOMING_VISIBLE_1_${step} claimId=${id}`;

test('actual Swift FIFO loader admits exact single context after original valid two-request control', { timeout: 30000 }, () => {
  const r = actualResults();
  assert.equal(r['loader-two'].accepted, true, 'Existing exact two-request decoder control must pass first');
  assert.equal(r['loader-two'].failed, false); assert.equal(r['loader-two'].loads, 1);
  assert.deepEqual(r['loader-two'].waits, [180]);
  assert.equal(r['loader-one'].accepted, true, 'Exact declared single request must pass the actual typed decoder');
  assert.equal(r['loader-one'].failed, false);
});

test('actual CaseFour single receiver emits exact claim marker after complete original branch transcript', { timeout: 30000 }, () => {
  const r = actualResults(), two = r['receiver-two'], one = r['receiver-one'];
  assert.deepEqual(two.markers, [marker(1, requestIds[0]), marker(2, requestIds[1])]);
  assert.equal(two.grants, 2); assert.equal(two.attempts, 2); assert.equal(two.decisions, 2); assert.equal(two.recover, 0);
  assert.equal(one.markers.length, 1); assert.equal(one.attempts, 1, 'Original complete branch reaches the action-delivery port');
  assert.equal(one.recover, 0); assert.equal(one.closes, 0);
  assert.deepEqual(one.markers, [marker(1, requestIds[0])], 'A plain legacy marker cannot satisfy the exact current Claim contract');
  assert.equal(one.grants, 1); assert.equal(one.decisions, 1);
});

test('actual typed receiver preserves original steps and waits for each exact host grant before decision', () => {
  const r = actualResults();
  const reverse = r['receiver-reversed'];
  assert.deepEqual(reverse.markers, [marker(2, requestIds[1]), marker(1, requestIds[0])]);
  assert.equal(reverse.decisions, 2); assert.equal(reverse.recover, 0);
  for (const name of ['receiver-one', 'receiver-two', 'receiver-reversed']) {
    const events = r[name].events.filter(value => ['visible', 'permit', 'attempt', 'decision'].includes(value));
    assert.deepEqual(events, Array.from({ length: r[name].decisions }, () => ['visible', 'permit', 'attempt', 'decision']).flat(), name);
  }
  for (const name of ['zero-assigned', 'missing-scope']) {
    assert.deepEqual(r[name].markers, []); assert.equal(r[name].attempts, 0); assert.equal(r[name].recover, 0);
  }
});

test('actual decoder rejects cardinality, scope and canonical identity corruptions without relaxing the old checks', () => {
  const r = actualResults();
  assert.equal(r['loader-opaque'].accepted, true, 'Actual transport validator keeps opaque Core IDs without inventing a UUID grammar');
  for (const [name, result] of Object.entries(r)) {
    if (!name.startsWith('invalid:')) continue;
    assert.equal(result.accepted, false, name); assert.equal(result.failed, true, name);
  }
  assert.equal(r['invalid:expected-zero'].loads, 0); assert.equal(r['invalid:expected-three'].loads, 0);
});

test('actual FIFO step rejects wrong or consumed visible heads and unassigned approval without entering decisions', () => {
  const r = actualResults();
  for (const [name, result] of Object.entries(r)) {
    if (name.startsWith('step:')) {
      assert.equal(result.matched, false, name); assert.equal(result.expansions, 0, name);
      assert.equal(result.attempts, 0); assert.equal(result.decisions, 0);
    }
    if (name.startsWith('receiver-invalid:')) {
      assert.deepEqual(result.markers, [], name); assert.equal(result.attempts, 0, name); assert.equal(result.grants, 0);
    }
  }
});


test('actual offline receiver mode never requests unpublished generic FIFO context', () => {
  const r = actualResults();
  assert.deepEqual(r['receiver-one'].markers, [marker(1, requestIds[0])]);
  assert.deepEqual(r['receiver-two'].markers, [marker(1, requestIds[0]), marker(2, requestIds[1])]);
  assert.equal(r['receiver-one'].decisions, 1); assert.equal(r['receiver-two'].decisions, 2);
  for (const name of ['mode:offline-android', 'mode:offline-web']) {
    const result = r[name]; assert.equal(result.recover, 0); assert.equal(result.closes, 0);
    assert.equal(result.loads, 0, name + ': offline host has no FIFO publisher');
    assert.deepEqual(result.markers, ['E2E: IOS_INCOMING_VISIBLE_1_1']);
    assert.deepEqual(result.permits, ['ios-approve-1']); assert.equal(result.grants, 1);
    assert.equal(result.attempts, 1); assert.equal(result.decisions, 1); assert.equal(result.failed, false);
  }
});

test('actual iOS offline early exit and generic refusal preserve their separate original authorities', () => {
  const r = actualResults(), offline = r['mode:offline-ios'], generic = r['mode:generic-unpublished'];
  assert.equal(offline.offlineReady, 1); assert.equal(offline.offlineAllowed, 1);
  assert.deepEqual(offline.permits, ['ios-offline-stop']); assert.deepEqual(offline.markers, []);
  assert.equal(offline.loads, 0); assert.equal(offline.attempts, 0); assert.equal(offline.recover, 0);
  assert.equal(offline.failed, false);
  assert(generic.loads > 0); assert.equal(generic.failed, true);
  assert.deepEqual(generic.markers, []); assert.equal(generic.attempts, 0); assert.equal(generic.grants, 0);
  assert.deepEqual(r['zero-assigned'].markers, []); assert.equal(r['zero-assigned'].attempts, 0);
});
