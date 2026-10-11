import test from 'node:test';
import assert from 'node:assert/strict';
import { markerScan, markerScanStream, ssClaimsInvalidationMarker } from './process-marker.mjs';

test('server SsClaims marker ignores runtime VaultName formatting and excludes device events', () => {
  const emittedShapes = [
    'Publishing state invalidation vault_name=VaultName("e2e vault+test (cycle 1)") scope=Some(SsClaims) revision=None',
    'Publishing state invalidation vault_name=[REDACTED_SECRET] scope=Some(SsClaims) revision=None',
    'Publishing state invalidation vault_name="runtime-vault" scope=Some(SsClaims) revision=None',
  ];

  for (const emitted of emittedShapes) {
    assert.equal(markerScan('', emitted, ssClaimsInvalidationMarker).matched, true);
  }
  assert.equal(
    markerScan('', 'Publishing state invalidation vault_name=VaultName("vault") scope=Some(Devices)', ssClaimsInvalidationMarker).matched,
    false,
  );
});

test('next-only literal marker survives a marker split across output chunks', () => {
  const marker = 'scope=Some(SsClaims)';
  const first = markerScan('', 'prefix scope=Some(Ss', marker);
  assert.equal(first.matched, false);

  const second = markerScan(first.tail, 'Claims) suffix', marker);
  assert.equal(second.matched, true);
});

test('next-only marker checks an oversized chunk before trimming its tail', () => {
  const marker = 'Publishing state invalidation';
  const oversizedChunk = `${marker}${'x'.repeat(140_000)}`;
  const result = markerScan('', oversizedChunk, marker);

  assert.equal(result.matched, true);
  assert.equal(result.tail.length, marker.length - 1);
});

test('next-only marker does not match output emitted before the waiter', () => {
  const marker = 'scope=Some(SsClaims)';
  const outputBeforeWaiter = `prefix ${marker} suffix`;
  const resultAfterWaiter = markerScan('', 'no relevant event', marker);

  assert.equal(outputBeforeWaiter.includes(marker), true);
  assert.equal(resultAfterWaiter.matched, false);
});

test('next-only marker survives interleaved stdout and stderr chunks without cross-stream matches', () => {
  const marker = 'Publishing state invalidation vault_name=VaultName("test") scope=Some(SsClaims)';
  let pendingByStream = {};

  const firstStdout = markerScanStream(pendingByStream, 'stdout', marker.slice(0, 36), marker);
  assert.equal(firstStdout.matched, false);
  pendingByStream = firstStdout.pendingByStream;

  const interleavedStderr = markerScanStream(pendingByStream, 'stderr', 'unrelated stderr output', marker);
  assert.equal(interleavedStderr.matched, false);
  pendingByStream = interleavedStderr.pendingByStream;

  const falseCrossStreamCompletion = markerScanStream(
    pendingByStream,
    'stderr',
    marker.slice(36),
    marker,
  );
  assert.equal(falseCrossStreamCompletion.matched, false);
  pendingByStream = falseCrossStreamCompletion.pendingByStream;

  const completedStdout = markerScanStream(pendingByStream, 'stdout', marker.slice(36), marker);
  assert.equal(completedStdout.matched, true);
});
