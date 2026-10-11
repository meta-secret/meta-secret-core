import assert from 'node:assert/strict';
import test from 'node:test';
import { reclaimBuildCaches } from './build-cache-cleanup.mjs';

test('preserves suite build outputs when cheaper cleanup reached the minimum, including forced cleanup', () => {
  const messages = [];
  const result = reclaimBuildCaches({ measureFreeGiB: () => 12, minimumGiB: 12,
    buildersIdle: () => { throw new Error('unnecessary process check'); },
    cleanups: [() => { throw new Error('must preserve caches'); }], report: (m) => messages.push(m) });
  assert.equal(result, 12);
  assert.match(messages[0], /preserved/);
});

test('measures after each fallback and preserves Cargo when an earlier fallback suffices', () => {
  const readings = [9, 10, 13];
  const calls = [];
  assert.equal(reclaimBuildCaches({ measureFreeGiB: () => readings.shift(), minimumGiB: 12,
    buildersIdle: () => true, cleanups: [() => calls.push('xcode'), () => calls.push('gradle'), () => calls.push('cargo')],
    report: () => {} }), 13);
  assert.deepEqual(calls, ['xcode', 'gradle']);
});

test('Cargo fallback runs only when all cheaper steps still leave insufficient space', () => {
  const readings = [9, 10, 11, 20];
  const calls = [];
  assert.equal(reclaimBuildCaches({ measureFreeGiB: () => readings.shift(), minimumGiB: 12,
    buildersIdle: () => true, cleanups: [() => calls.push('xcode'), () => calls.push('gradle'), () => calls.push('cargo')],
    report: () => {} }), 20);
  assert.deepEqual(calls, ['xcode', 'gradle', 'cargo']);
});

test('active or unverifiable builds prevent destructive cache fallback', () => {
  assert.equal(reclaimBuildCaches({ measureFreeGiB: () => 9, minimumGiB: 12,
    buildersIdle: () => false, cleanups: [() => { throw new Error('build active'); }], report: () => {} }), 9);
});
