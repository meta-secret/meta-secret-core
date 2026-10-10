import assert from 'node:assert/strict';
import test from 'node:test';
import { createHarnessShutdown, formatHarnessShutdownError } from './harness-shutdown.mjs';

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}

test('signal and concurrent actor failure share diagnostics before the cleanup snapshot', async () => {
  const screenshotStarted = deferred();
  const screenshotFinished = deferred();
  const events = [];
  const tracked = new Set(['actor']);
  const shutdown = createHarnessShutdown({
    cancelActors() { events.push('cancel'); },
    async captureDiagnostics() {
      shutdown.assertCanSpawn();
      tracked.add('screenshot-adb');
      events.push('screenshot-start');
      screenshotStarted.resolve();
      await screenshotFinished.promise;
      tracked.delete('screenshot-adb');
      events.push('screenshot-end');
    },
    async cleanup() {
      assert.deepEqual([...tracked], ['actor']);
      events.push('snapshot');
      shutdown.assertCanSpawn(); // Docker/environment restore is permitted.
    },
  });
  const signal = shutdown.finish({ error: new Error('SIGTERM'), diagnostics: true });
  await screenshotStarted.promise;
  const actorFailure = shutdown.finish({ error: new Error('actor exited'), diagnostics: true });
  assert.equal(actorFailure, signal);
  assert.throws(() => shutdown.assertCanSpawn(), /shutdown is already in progress/);
  assert.deepEqual(events, ['cancel', 'screenshot-start']);
  screenshotFinished.resolve();
  await Promise.all([signal, actorFailure]);
  assert.deepEqual(events, ['cancel', 'screenshot-start', 'screenshot-end', 'snapshot']);
});

test('diagnostic failure still cleans up once and its shared rejection is observable', async () => {
  let cleanups = 0;
  const screenshotError = new Error('device offline');
  const shutdown = createHarnessShutdown({
    cancelActors() {},
    captureDiagnostics() { throw screenshotError; },
    cleanup() { cleanups++; },
  });
  const first = shutdown.finish({ diagnostics: true });
  const second = shutdown.finish({ diagnostics: true });
  assert.equal(first, second);
  await assert.rejects(first, (error) => error instanceof AggregateError && error.errors.includes(screenshotError) && error.cause === screenshotError);
  await assert.rejects(second, /Harness shutdown failed/);
  assert.equal(cleanups, 1);
});

test('successful cleanup does not start late failure diagnostics after the snapshot', async () => {
  let captures = 0;
  const shutdown = createHarnessShutdown({
    cancelActors() {}, captureDiagnostics() { captures++; }, cleanup() {},
  });
  const completed = shutdown.finish();
  assert.equal(shutdown.finish({ diagnostics: true }), completed);
  await completed;
  assert.equal(captures, 0);
});

test('a delayed diagnostic continuation cannot launch after cleanup starts', async () => {
  const releaseLateDiagnostic = deferred();
  let lateDiagnostic;
  const shutdown = createHarnessShutdown({
    cancelActors() {},
    captureDiagnostics() {
      lateDiagnostic = releaseLateDiagnostic.promise.then(() => {
        assert.throws(() => shutdown.assertCanSpawn(), /shutdown is already in progress/);
      });
    },
    async cleanup() {
      releaseLateDiagnostic.resolve();
      await lateDiagnostic;
    },
  });
  await shutdown.finish({ diagnostics: true });
});

test('shutdown reporting includes nested errors and causes after redaction', () => {
  const error = new AggregateError([
    new Error('device offline: SECRET_VALUE'),
    new Error('environment fingerprint changed', { cause: new Error('mode restore failed: SECRET_VALUE') }),
  ], 'Harness shutdown failed');
  const report = formatHarnessShutdownError(error, (text) => text.replaceAll('SECRET_VALUE', '[REDACTED]'));
  assert.match(report, /shutdown.errors\[0\]: Error: device offline: \[REDACTED\]/);
  assert.match(report, /shutdown.errors\[1\]: Error: environment fingerprint changed/);
  assert.match(report, /shutdown.errors\[1\].cause: Error: mode restore failed: \[REDACTED\]/);
  assert.equal(report.includes('SECRET_VALUE'), false);
});

test('shutdown reporting handles cyclic causes without dropping the original error', () => {
  const error = new Error('cleanup failure');
  error.cause = error;
  const report = formatHarnessShutdownError(error, (text) => text);
  assert.match(report, /Error: cleanup failure/);
  assert.match(report, /cause: \[already reported\]/);
});
