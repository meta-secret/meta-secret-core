import { test } from 'node:test';
import assert from 'node:assert/strict';
import { actualGlobalPrivacyResults } from './ios-global-privacy-snapshot-fixture.mjs';

function noActions(result) {
  for (const key of ['actions', 'clipboard', 'auth', 'navigation']) assert.equal(result[key], 0, key);
}
function clean(result, assertions = 3) {
  assert.deepEqual(result.failures, []); assert.equal(result.assertions, assertions); noActions(result);
}
function rejects(result) {
  assert(result.failures.length > 0, 'An incomplete or leaking global observation cannot report clean');
  noActions(result);
}
function singleSnapshot(result, count = 1) {
  assert.equal(result.captures, count, 'Exactly one fresh capture per public invocation');
  for (const key of ['liveExists', 'liveLabels', 'liveValues', 'liveIdentifiers', 'liveQueries']) {
    assert.equal(result[key], 0, `Snapshot observation must not issue per-node live RPCs: ${key}`);
  }
  noActions(result);
}

test('baseline logical controls retain clean nested empty nil and non-String semantics', { timeout: 35000 }, () => {
  const results = actualGlobalPrivacyResults();
  for (const mode of ['clean', 'nested', 'empty', 'nil-value', 'non-string-value', 'case-sensitive', 'near-prefix']) {
    clean(results[mode]);
  }
});

test('baseline logical controls retain exact label and String-value contains assertions', () => {
  const results = actualGlobalPrivacyResults();
  for (const mode of ['label-leak', 'value-leak', 'nested-value']) {
    const result = results[mode]; rejects(result); assert.equal(result.assertions, 3);
    assert.deepEqual(result.failures, ['Private plaintext appears in the foreground accessibility tree']);
  }
});

test('baseline logical controls retain both global private-prefix assertions independently', () => {
  const results = actualGlobalPrivacyResults();
  for (const mode of ['value-prefix', 'copy-prefix']) {
    rejects(results[mode]); assert.deepEqual(results[mode].failures, ['EQUAL']);
    assert.equal(results[mode].assertions, 3);
  }
  for (const mode of ['both-prefixes', 'duplicate-private']) {
    rejects(results[mode]); assert.deepEqual(results[mode].failures, ['EQUAL', 'EQUAL']);
    assert.equal(results[mode].assertions, 3);
  }
});

test('baseline logical controls exclude root attributes and include every window and offscreen descendant', () => {
  const results = actualGlobalPrivacyResults(); clean(results['root-only']);
  for (const mode of ['other-window-label', 'offscreen-value']) {
    rejects(results[mode]); assert.equal(results[mode].assertions, 3);
    assert.deepEqual(results[mode].failures, ['Private plaintext appears in the foreground accessibility tree']);
  }
});

test('cost semantic RED requires one fresh capture and no per-node live privacy queries', () => {
  const results = actualGlobalPrivacyResults();
  clean(results.clean); singleSnapshot(results.clean);
  clean(results.nested); singleSnapshot(results.nested);
  clean(results.empty); singleSnapshot(results.empty);
});

test('actual global snapshot accepts exactly 8192 descendants with the root excluded', () => {
  const result = actualGlobalPrivacyResults()['limit-8192']; clean(result); singleSnapshot(result);
  assert.equal(result.snapshotNodes, 8192);
});

test('actual global snapshot refuses descendant overflow and a large frontier without partial clean success', () => {
  const results = actualGlobalPrivacyResults();
  for (const mode of ['limit-8193', 'large-frontier']) {
    const result = results[mode]; rejects(result); singleSnapshot(result);
    assert(result.snapshotNodes <= 8192, 'Reject before reading an out-of-budget descendant');
    assert.equal(result.assertions, 0, 'Incomplete observations must not feed the three clean-data assertions');
  }
});

test('actual global snapshot fails closed on public capture errors without retry or body logging', () => {
  const results = actualGlobalPrivacyResults();
  for (const mode of ['capture-throw', 'capture-unavailable', 'capture-incomplete']) {
    const result = results[mode]; rejects(result); singleSnapshot(result);
    assert.equal(result.snapshotReads, 0); assert.equal(result.assertions, 0);
  }
});

test('actual global snapshot includes capture time and rejects expired capture or walk observations', () => {
  const results = actualGlobalPrivacyResults();
  for (const mode of ['capture-late', 'capture-spent-budget', 'walk-expired', 'attribute-late', 'postwalk-late']) {
    const result = results[mode]; rejects(result); singleSnapshot(result);
    assert.equal(result.assertions, 0, 'Late data must not become a clean observation');
    assert(result.clockReads >= 2, 'The monotonic budget must be measured across capture and completion');
  }
  const timely = results['under-deadline']; clean(timely); singleSnapshot(timely);
});

test('actual global snapshot is fresh for both invocations and cannot reuse clean cached data', () => {
  const results = actualGlobalPrivacyResults();
  const twice = results['two-clean-calls']; clean(twice, 6); singleSnapshot(twice, 2);
  const changed = results['fresh-two-calls']; rejects(changed); singleSnapshot(changed, 2);
  assert.equal(changed.firstFailures, 0); assert.equal(changed.firstAssertions, 3);
  assert.equal(changed.assertions, 6);
  assert.deepEqual(changed.failures, ['Private plaintext appears in the foreground accessibility tree']);
});

test('actual global snapshot uses immutable descendant presence rather than a later live exists sample', () => {
  const results = actualGlobalPrivacyResults();
  clean(results['immutable-clean']); singleSnapshot(results['immutable-clean']);
  for (const mode of ['immutable-leak', 'snapshot-presence']) {
    rejects(results[mode]); singleSnapshot(results[mode]);
    assert.deepEqual(results[mode].failures, ['Private plaintext appears in the foreground accessibility tree']);
  }
});

test('every actual-source path records failures without native input clipboard auth or navigation', () => {
  for (const result of Object.values(actualGlobalPrivacyResults())) noActions(result);
});
