import { test } from 'node:test';
import assert from 'node:assert/strict';
import { actualSameRequestResults } from './ios-same-request-show-fixture.mjs';

function assertOriginalShowTail(result, recovers, valueRequires, copyLookups) {
  assert.deepEqual(result.failures, []);
  assert.equal(result.recovers, recovers); assert.equal(result.shows, 1);
  assert.equal(result.pins, 1); assert.equal(result.valueExists, true);
  assert.equal(result.valueRequires, valueRequires); assert.equal(result.copyLookups, copyLookups);
  assert.equal(result.inputsMatched, true);
  assert(result.events.indexOf('pin') > result.events.indexOf('show-input'));
  assert(result.events.lastIndexOf('require-value') > result.events.indexOf('pin'));
}

test('actual CaseFour same-request Show completes owner gates before rejecting a repeated full-name lookup', { timeout: 30000 }, () => {
  const result = actualSameRequestResults()['same-request'];
  assertOriginalShowTail(result, 1, 1, 0);
  assert(result.events.indexOf('show-input') > result.events.indexOf('recover-input'));
  assert.equal(result.lookups, 1, 'One actual Recover -> Show must resolve the full name exactly once');
});

test('actual default Show retains its full-name lookup and original nil-expected value tail', () => {
  const result = actualSameRequestResults()['default-show'];
  assertOriginalShowTail(result, 0, 1, 0);
  assert.equal(result.lookups, 1); assert.equal(result.predicates, 1);
});

test('actual optional expected Show retains plaintext predicate and Copy oracle in the same owning tail', () => {
  const result = actualSameRequestResults()['expected-show'];
  assertOriginalShowTail(result, 0, 2, 1);
  assert.equal(result.lookups, 1); assert.equal(result.predicates, 2);
});

function assertRejectedBeforeShow(result, lookups = 1, recovers = 1, reaches = 0) {
  assert(result.failures.length > 0, 'Actual guard must report its refusal');
  assert.equal(result.token, false); assert.equal(result.lookups, lookups);
  assert.equal(result.recoverAttempts, recovers); assert.equal(result.exactReaches, reaches);
  assert.equal(result.showAttempts, 0, 'Count every attempted Show input before any fake abort guard');
  assert.equal(result.tapAttempts, recovers, 'No extra attempted input may be masked by the assertion port');
}

test('actual CaseFour binds retained app selector namespace cycle name and full ID without map fallback', () => {
  const results = actualSameRequestResults();
  assertRejectedBeforeShow(results.missing, 0, 0);
  for (const mode of ['cycle', 'name', 'app', 'selector-reset', 'app-namespace', 'scoped-namespace',
    'lost-namespace', 'both-namespace', 'selected-id', 'map-id']) {
    assert.equal(results[mode].issued, true, mode); assertRejectedBeforeShow(results[mode]);
  }
});

test('actual pending gates alone issue tokens and a failed new request revokes the earlier one', () => {
  const results = actualSameRequestResults();
  for (const mode of ['missing-namespace', 'invalid-namespace', 'pending', 'pending-delete', 'pending-primary',
    'pending-duplicate', 'pending-empty-id', 'pending-map']) {
    assert.equal(results[mode].issued, false, mode); assertRejectedBeforeShow(results[mode]);
  }
  assert.equal(results.pending.firstFaultMatches, true);
  assert.equal(results.pending.failures[0], 'OWNED_PENDING');
  assert.equal(results['new-request-failed'].issued, true);
  assertRejectedBeforeShow(results['new-request-failed'], 2, 2);
  assert.equal(results['new-request-failed'].firstFaultMatches, true);
});

test('actual selected Show requires fresh global exact counts and same-card route ownership', () => {
  const results = actualSameRequestResults();
  for (const mode of ['missing-card', 'missing-route', 'missing-list', 'missing-name', 'renamed', 'duplicate-card', 'duplicate-name',
    'duplicate-primary', 'prefix-collision',
    'duplicate-route', 'duplicate-list', 'wrong-primary-type']) assertRejectedBeforeShow(results[mode], 1, 1, 1);
  for (const mode of ['detached-name', 'detached-primary', 'detached-card']) assertAssociatedShow(results[mode]);
});

test('actual selected Show revalidates enabled label and structure after its unchanged readiness wait', () => {
  const results = actualSameRequestResults();
  for (const mode of ['ready', 'wrong-label', 'after-ready-name', 'after-ready-id', 'after-ready-primary', 'after-ready-disabled', 'after-ready-label']) {
    assertRejectedBeforeShow(results[mode], 1, 1, 1);
  }
  assert.equal(results.ready.firstFaultMatches, true); assert.equal(results.ready.failures[0], 'OWNED_READY');
});

test('actual request continuation survives pending Home and incoming selection and never retargets a later same-name ID', () => {
  const results = actualSameRequestResults();
  for (const mode of ['home-incoming', 'unrelated-same-name']) {
    const result = results[mode];
    assert.deepEqual(result.failures, [], mode); assert.equal(result.issued, true); assert.equal(result.token, false);
    assert.equal(result.lookups, 1); assert.equal(result.exactReaches, 1);
    assert.equal(result.recoverAttempts, 1); assert.equal(result.showAttempts, 1); assert.equal(result.tapAttempts, 2);
    assert.equal(result.shows, 1); assert.equal(result.valueRequires, 1); assert.equal(result.copyLookups, 0);
  }
});

test('actual token is spent before dispatch and repeated reentrant or terminal calls cannot replay Show', () => {
  const results = actualSameRequestResults();
  for (const mode of ['repeated', 'reentrant']) {
    const result = results[mode];
    assert(result.failures.length > 0); assert.equal(result.token, false);
    assert.equal(result.lookups, 1); assert.equal(result.exactReaches, 1);
    assert.equal(result.tapAttempts, 2); assert.equal(result.recoverAttempts, 1); assert.equal(result.showAttempts, 1);
  }
  assert.equal(results.reentrant.tokenAtInput, false);
  for (const mode of ['setup', 'teardown', 'revealed-close']) assertRejectedBeforeShow(results[mode]);
  const next = results['next-cycle']; assert.deepEqual(next.failures, []); assert.equal(next.token, false);
  assert.equal(next.lookups, 2); assert.equal(next.recoverAttempts, 2); assert.equal(next.showAttempts, 2);
  assert.equal(next.tapAttempts, 4); assert.equal(next.exactReaches, 2);
});

test('actual owner reach auth and value faults preserve the primary port fault without action replay', () => {
  const results = actualSameRequestResults();
  assertRejectedBeforeShow(results.reach, 1, 1, 1);
  for (const [mode, first] of [['reach', 'OWNED_REACH'], ['auth', 'OWNED_AUTH'], ['value', 'OWNED_VALUE']]) {
    const result = results[mode];
    assert.equal(result.failures[0], first); assert.equal(result.firstFaultMatches, true);
    assert.equal(result.lookups, 1); assert.equal(result.recoverAttempts, 1); assert.equal(result.exactReaches, 1);
    if (mode !== 'reach') {
      assert.equal(result.tapAttempts, 2); assert.equal(result.showAttempts, 1); assert.equal(result.pins, 1);
      assert.equal(result.valueRequires, 1);
    }
  }
});

function scopeResult(mode) { return actualSameRequestResults()[`scope-${mode}`]; }
function oneScopeRecord(mode) {
  const result = scopeResult(mode);
  assert.equal(result.diagnostics.length, 1, mode); assert.equal(result.diagnosticAttempts, 1, mode);
  const line = result.diagnostics[0]; assert(Buffer.byteLength(line, 'utf8') <= 512, mode);
  const prefix = 'LB_SHOW_SCOPE_V1 '; assert(line.startsWith(prefix));
  const { targetCounts, targetWindows, sameWindowAll, ...row } = JSON.parse(line.slice(prefix.length));
  assert.deepEqual(Object.keys(row).sort(), ['v', 'coverage', 'routeList', 'listCard', 'cardName', 'cardPrimary',
    'routeWindow', 'listWindow', 'sameWindow', 'scope'].sort());
  assert.equal(row.v, 1); assert(['SNAPSHOT', 'UNAVAILABLE', 'NO_SNAPSHOT'].includes(row.coverage));
  for (const key of ['routeList', 'listCard', 'cardName', 'cardPrimary']) assert(['ZERO', 'ONE', 'MANY', 'UNKNOWN'].includes(row[key]), key);
  for (const key of ['routeWindow', 'listWindow']) assert(['PRESENT', 'MISSING', 'UNKNOWN'].includes(row[key]), key);
  assert(['TRUE', 'FALSE', 'UNKNOWN'].includes(row.sameWindow)); assert(['VALID', 'INVALID', 'UNKNOWN'].includes(row.scope));
  return row;
}

test('actual same-window sibling route list completes one exact-request Show after the real snapshot helper succeeds', () => {
  const result = scopeResult('sibling');
  assert.equal(result.probe, true, 'Old actual public snapshot helper must compile and validate the controlled scope first');
  assert.equal(result.issued, true); assert.equal(result.lookups, 1); assert.equal(result.recovers, 1);
  assert.equal(result.recoverAttempts, 1); assert.equal(result.exactReaches, 1);
  assert.equal(result.showAttempts, 1, 'Valid same-window sibling scope must permit one Show; old ancestry guard refuses before input');
  assert.deepEqual(result.failures, []); assert.equal(result.shows, 1); assert.equal(result.tapAttempts, 2);
  assert.equal(result.pins, 1); assert.equal(result.valueRequires, 1);
  assert.equal(result.snapshotCalls, 3, 'One successful public probe plus exactly one snapshot per selected check');
  assert.equal(result.diagnosticAttempts, 0); assert.deepEqual(result.diagnostics, []);
  const reviewed = actualSameRequestResults()['detached-list'];
  assert.deepEqual(reviewed.failures, []); assert.equal(reviewed.showAttempts, 1); assert.equal(reviewed.lookups, 1);
});

test('actual scope identity refuses distinct window nodes even with equal CGRect values', () => {
  const result = scopeResult('equal-window-rectangles');
  assert.equal(result.probe, true, 'The actual old frame-only snapshot helper accepts this shape; identity must strengthen it');
  assertRejectedBeforeShow(result, 1, 1, 1);
  const row = oneScopeRecord('equal-window-rectangles');
  assert.equal(row.coverage, 'SNAPSHOT'); assert.equal(row.routeWindow, 'PRESENT');
  assert.equal(row.listWindow, 'PRESENT'); assert.equal(row.sameWindow, 'FALSE');
});

test('actual snapshot helper preserves foreign missing duplicate nonfinite clip budget and error refusals', () => {
  for (const mode of ['foreign-window', 'missing-window', 'missing-snapshot-route', 'duplicate-snapshot-route',
    'missing-snapshot-list', 'duplicate-snapshot-list', 'nonfinite-root', 'nonfinite-window', 'nonfinite-route',
    'nonfinite-list', 'empty-clip', 'node-budget', 'snapshot-error']) {
    const result = scopeResult(mode); assertRejectedBeforeShow(result, 1, 1, 1);
    const row = oneScopeRecord(mode); assert.equal(row.coverage, 'UNAVAILABLE', mode);
    for (const key of ['routeList', 'listCard', 'cardName', 'cardPrimary', 'routeWindow', 'listWindow', 'sameWindow', 'scope']) {
      assert.equal(row[key], 'UNKNOWN', mode + ':' + key);
    }
  }
});

test('actual second selected check uses a fresh snapshot and rejects changed window or clip before input', () => {
  for (const mode of ['after-ready-window', 'after-ready-clip']) {
    const result = scopeResult(mode); assertRejectedBeforeShow(result, 1, 1, 1);
    assert.equal(result.snapshotCalls, 2, 'Exactly one actual snapshot per selected check, with no diagnostic reread');
    oneScopeRecord(mode);
  }
});

test('actual refusal diagnostics describe all four relations without inventing unobserved snapshot facts', () => {
  assertAssociatedShow(scopeResult('all-relations'));
  assert.equal(scopeResult('all-relations').snapshotCalls, 2);
  assert.equal(scopeResult('all-relations').diagnosticAttempts, 0);
  const absent = scopeResult('no-snapshot'); assertRejectedBeforeShow(absent, 1, 1, 1);
  assert.equal(absent.snapshotCalls, 0, 'A failed pre-snapshot guard must not fetch a tree only for diagnostics');
  const unknown = oneScopeRecord('no-snapshot'); assert.equal(unknown.coverage, 'NO_SNAPSHOT');
  for (const key of ['routeList', 'listCard', 'cardName', 'cardPrimary', 'routeWindow', 'listWindow', 'sameWindow', 'scope']) assert.equal(unknown[key], 'UNKNOWN');
});

test('actual dropped diagnostic output preserves the original refusal and never admits a Show attempt', () => {
  const result = scopeResult('logger-drop'); assertRejectedBeforeShow(result, 1, 1, 1);
  assert.equal(result.diagnosticAttempts, 1); assert.equal(result.diagnosticDropped, true);
  assert.deepEqual(result.diagnostics, []);
  assert.equal(result.failures[0], 'Exact same-request Show target is unavailable');
});


function metadataRecord(mode) {
  oneScopeRecord(mode); // Preserve original schema/privacy/size assertions on its unchanged projection.
  return JSON.parse(scopeResult(mode).diagnostics[0].slice('LB_SHOW_SCOPE_V1 '.length));
}
function assertMetadataShape(row) {
  assert.deepEqual(Object.keys(row).sort(), ['v', 'coverage', 'routeList', 'listCard', 'cardName', 'cardPrimary',
    'routeWindow', 'listWindow', 'sameWindow', 'scope', 'targetCounts', 'targetWindows', 'sameWindowAll'].sort());
  assert.equal(row.targetCounts.length, 5); assert.equal(row.targetWindows.length, 5);
  for (const count of row.targetCounts) assert(['ZERO', 'ONE', 'MANY', 'UNKNOWN'].includes(count));
  for (const presence of row.targetWindows) assert(['PRESENT', 'MISSING', 'UNKNOWN'].includes(presence));
  assert(['TRUE', 'FALSE', 'UNKNOWN'].includes(row.sameWindowAll));
}

test('association causal RED: same-request Show admits the observed five-target flat scope exactly once', () => {
  const result = scopeResult('metadata-flat-all');
  assert.equal(result.issued, true); assert.equal(result.token, false); assert.equal(result.lookups, 1);
  assert.equal(result.exactReaches, 1); assert.equal(result.recovers, 1); assert.equal(result.recoverAttempts, 1);
  if (result.showAttempts === 0) {
    assertRejectedBeforeShow(result, 1, 1, 1); assert.equal(result.snapshotCalls, 1);
    const row = metadataRecord('metadata-flat-all'); assertMetadataShape(row);
    assert.equal(row.coverage, 'SNAPSHOT'); assert.equal(row.scope, 'VALID');
    for (const relation of ['routeList', 'listCard', 'cardName', 'cardPrimary']) assert.equal(row[relation], 'ZERO');
    assert.deepEqual(row.targetCounts, ['ONE', 'ONE', 'ONE', 'ONE', 'ONE']);
    assert.deepEqual(row.targetWindows, ['PRESENT', 'PRESENT', 'PRESENT', 'PRESENT', 'PRESENT']);
    assert.equal(row.sameWindowAll, 'TRUE');
  }
  assert.equal(result.showAttempts, 1, 'Observed five-target current immutable scope must permit one exact-request Show; f67 attempts0');
  assert.deepEqual(result.failures, []); assert.equal(result.shows, 1); assert.equal(result.tapAttempts, 2);
  assert.equal(result.pins, 1); assert.equal(result.valueRequires, 1); assert.equal(result.snapshotCalls, 2);
});

test('metadata actual snapshot distinguishes missing duplicate foreign and unwindowed exact targets without input', () => {
  for (const [target, index] of [['card', 2], ['name', 3], ['primary', 4]]) {
    for (const fault of ['missing', 'duplicate', 'foreign', 'unwindowed']) {
      const mode = `metadata-${fault}-${target}`, result = scopeResult(mode);
      assertRejectedBeforeShow(result, 1, 1, 1); assert.equal(result.snapshotCalls, 1);
      const row = metadataRecord(mode); assertMetadataShape(row);
      const counts = ['ONE', 'ONE', 'ONE', 'ONE', 'ONE'];
      const windows = ['PRESENT', 'PRESENT', 'PRESENT', 'PRESENT', 'PRESENT'];
      if (fault === 'missing' || fault === 'duplicate') {
        counts[index] = fault === 'missing' ? 'ZERO' : 'MANY'; windows[index] = 'UNKNOWN';
      }
      if (fault === 'unwindowed') windows[index] = 'MISSING';
      assert.deepEqual(row.targetCounts, counts); assert.deepEqual(row.targetWindows, windows);
      assert.equal(row.sameWindowAll, fault === 'foreign' ? 'FALSE' : 'UNKNOWN');
      assert.equal(row.sameWindow, 'TRUE', 'Original route/list decision remains unchanged');
    }
  }
});

test('metadata unavailable paths retain UNKNOWN and never add a diagnostic snapshot or action', () => {
  for (const mode of ['no-snapshot', 'snapshot-error', 'node-budget', 'missing-window',
    'missing-snapshot-route', 'duplicate-snapshot-route', 'missing-snapshot-list', 'duplicate-snapshot-list',
    'nonfinite-root', 'nonfinite-window', 'nonfinite-route', 'nonfinite-list', 'empty-clip']) {
    const result = scopeResult(mode); assertRejectedBeforeShow(result, 1, 1, 1);
    const row = metadataRecord(mode); assertMetadataShape(row);
    assert.deepEqual(row.targetCounts, ['UNKNOWN', 'UNKNOWN', 'UNKNOWN', 'UNKNOWN', 'UNKNOWN']);
    assert.deepEqual(row.targetWindows, ['UNKNOWN', 'UNKNOWN', 'UNKNOWN', 'UNKNOWN', 'UNKNOWN']);
    assert.equal(row.sameWindowAll, 'UNKNOWN');
    assert.equal(result.snapshotCalls, mode === 'no-snapshot' ? 0 : 1);
  }
});


function assertAssociatedShow(result) {
  assert.deepEqual(result.failures, []); assert.equal(result.issued, true); assert.equal(result.token, false);
  assert.equal(result.lookups, 1); assert.equal(result.exactReaches, 1);
  assert.equal(result.recoverAttempts, 1); assert.equal(result.showAttempts, 1); assert.equal(result.tapAttempts, 2);
  assert.equal(result.recovers, 1); assert.equal(result.shows, 1); assert.equal(result.pins, 1);
  assert.equal(result.valueRequires, 1);
}

test('association exact tag roles reject contrary complete incomplete and ambiguous owners without input', () => {
  for (const mode of ['association-foreign-card', 'association-foreign-name', 'association-foreign-primary',
    'association-incomplete-owner', 'association-role-alias']) {
    const result = scopeResult(mode); assertRejectedBeforeShow(result, 1, 1, 1);
    assert.equal(result.snapshotCalls, 1); assert.equal(result.inputsMatched, true);
  }
  const name = scopeResult('association-name-tag-not-owner');
  assertAssociatedShow(name); assert.equal(name.inputsMatched, true); assert.equal(name.snapshotCalls, 2);
});

test('association current immutable name type and explicit owner are rechecked after readiness before input', () => {
  for (const mode of ['association-snapshot-name', 'association-snapshot-type',
    'association-after-ready-owner', 'association-after-ready-snapshot-name', 'association-after-ready-snapshot-type']) {
    const result = scopeResult(mode); assertRejectedBeforeShow(result, 1, 1, 1);
    assert.equal(result.snapshotCalls, mode.includes('after-ready') ? 2 : 1);
  }
});

// API-fidelity addition only; all pre-existing owner/scope/privacy/oracle tests above unchanged.
test('fixture optional input callback true executes once before actual modeled input', () => {
  const result=actualSameRequestResults()['callback-allow'];
  assert.deepEqual(result.failures,[]);assert.equal(result.callbacks,1);
  assert.equal(result.tapAttempts,1);assert.equal(result.showAttempts,1);assert.equal(result.shows,1);
  assert.equal(result.recovers,0);assert.equal(result.pins,0);assert.equal(result.valueExists,true);
  assert.deepEqual(result.events,['callback','show-input']);
});
test('fixture optional input callback refusal preserves attempt counters and stops disclosure', () => {
  const result=actualSameRequestResults()['callback-refuse'];
  assert.equal(result.callbacks,1);assert.equal(result.tapAttempts,1);assert.equal(result.showAttempts,1);
  assert.equal(result.shows,0);assert.equal(result.recovers,0);assert.equal(result.pins,0);assert.equal(result.valueExists,false);
  assert.deepEqual(result.events,['callback']);
  assert.deepEqual(result.failures,['Fresh action scope changed at the actual input boundary']);
});
test('fixture optional input callback cannot override original primary fault admission', () => {
  const result=actualSameRequestResults()['callback-prior-fault'];
  assert.equal(result.callbacks,0);assert.equal(result.tapAttempts,1);assert.equal(result.showAttempts,1);
  assert.equal(result.shows,0);assert.equal(result.recovers,0);assert.equal(result.pins,0);assert.equal(result.valueExists,false);
  assert.deepEqual(result.events,[]);assert.equal(result.failures[0],'OWNED_CALLBACK_PRIMARY');assert.equal(result.firstFaultMatches,true);
});
