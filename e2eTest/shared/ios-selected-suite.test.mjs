import assert from 'node:assert/strict';
import test from 'node:test';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createIosSelectedSuiteActor, superviseIosSelectedSuite } from './ios-selected-suite.mjs';

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
};

function watchedFixture({ ignoreTerm = false, ignoreKill = false } = {}) {
  const processResult = deferred();
  const closed = deferred();
  const markers = new Map();
  const events = [];
  const failures = [];
  const child = { exitCode: null, signalCode: null, kill(signal) {
    events.push(signal);
    if ((signal === 'SIGTERM' && ignoreTerm) || (signal === 'SIGKILL' && ignoreKill)) return true;
    child.signalCode = signal;
    processResult.reject(new Error(`xcodebuild exited with ${signal}`));
    closed.resolve({ code: null, signal });
    return true;
  } };
  const watched = {
    child, result: processResult.promise, closed: closed.promise,
    waitForMarker(marker, deadline) {
      assert(deadline > 0);
      const pending = deferred(); markers.set(marker, pending); return pending.promise;
    },
    cancelPendingWaits(error) {
      events.push('cancel-pending'); failures.push(error);
      for (const pending of markers.values()) pending.reject(error);
      markers.clear();
    },
    outputTail: () => 'scoped redacted actor output',
  };
  return { watched, events, failures,
    emit(marker) { const pending = markers.get(marker); assert(pending, `Unarmed marker: ${marker}`); markers.delete(marker); pending.resolve(); },
    failProcess(error) {
      child.exitCode = 65;
      processResult.reject(error);
      for (const pending of markers.values()) pending.reject(error);
      markers.clear(); closed.resolve({ code: 65, signal: null });
    },
    finishNaturally(code = 0) {
      events.push('natural-exit');
      child.exitCode = code;
      if (code) processResult.reject(new Error(`xcodebuild naturally exited ${code}`));
      else processResult.resolve({ stdout: '', stderr: '' });
      closed.resolve({ code, signal: null });
    },
    finishWithSignal(error) {
      child.signalCode = 'SIGABRT';
      processResult.reject(error);
      closed.resolve({ code: null, signal: 'SIGABRT' });
    },
  };
}

test('failed selected suite cancels missing action waits with the first cause, joins wrapper and retains xcresult', { timeout: 1_000 }, async () => {
  const directory = mkdtempSync(join(tmpdir(), 'ios-suite-failure-'));
  const bundle = join(directory, 'failure.xcresult'); writeFileSync(bundle, 'retained diagnostic fixture');
  const fixture = watchedFixture();
  let cleanupCalls = 0;
  const result = superviseIosSelectedSuite({ watched: fixture.watched, selector: 'Actor/registration', timeoutMs: 360_000,
    naturalExitMs: 0,
    onPassed: () => { cleanupCalls += 1; rmSync(bundle); },
  });
  const missingAction = fixture.watched.waitForMarker('E2E: QR_GENERATED', 180_000);
  const actionRejected = assert.rejects(missingAction, (error) => error.code === 'IOS_SELECTED_SUITE_FAILED');
  fixture.emit("Test Suite 'Selected tests' failed");
  try {
    await assert.rejects(result, (error) => {
      assert.equal(error.code, 'IOS_SELECTED_SUITE_FAILED');
      assert.match(error.message, /Actor\/registration/);
      assert.equal(error, fixture.failures[0], 'All actor waiters retain the exact terminal failure object');
      assert.doesNotMatch(error.message, /SIGTERM/);
      return true;
    });
    await actionRejected;
    assert.deepEqual(fixture.events, ['cancel-pending', 'SIGTERM']);
    assert.equal(fixture.watched.child.signalCode, 'SIGTERM');
    assert.equal(cleanupCalls, 0); assert(existsSync(bundle));
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('passing suite still consumes intentional SIGTERM and cleans up only after owned close', async () => {
  const fixture = watchedFixture();
  let cleanupCalls = 0;
  const result = superviseIosSelectedSuite({ watched: fixture.watched, selector: 'Actor/success', timeoutMs: 1000,
    naturalExitMs: 0,
    onPassed: () => { assert.equal(fixture.watched.child.signalCode, 'SIGTERM'); cleanupCalls += 1; },
  });
  fixture.emit("Test Suite 'Selected tests' passed");
  assert.deepEqual(await result, { stdout: 'scoped redacted actor output', stderr: '' });
  assert.equal(cleanupCalls, 1);
  assert.deepEqual(fixture.events, ['cancel-pending', 'SIGTERM']);
});

test('a failed suite with a lingering wrapper uses bounded SIGKILL without replacing the suite error', { timeout: 1000 }, async () => {
  const fixture = watchedFixture({ ignoreTerm: true });
  const result = superviseIosSelectedSuite({ watched: fixture.watched, selector: 'Actor/hang', timeoutMs: 360_000,
    closeTimeoutMs: 5, killTimeoutMs: 50,
    naturalExitMs: 5,
  });
  fixture.emit("Test Suite 'Selected tests' failed");
  await assert.rejects(result, (error) => error.code === 'IOS_SELECTED_SUITE_FAILED');
  assert.deepEqual(fixture.events, ['cancel-pending', 'SIGTERM', 'SIGKILL']);
});

test('failed owned cleanup preserves both errors with the selected suite as the first cause', { timeout: 1000 }, async () => {
  const fixture = watchedFixture({ ignoreTerm: true, ignoreKill: true });
  const result = superviseIosSelectedSuite({ watched: fixture.watched, selector: 'Actor/unresponsive', timeoutMs: 1000,
    closeTimeoutMs: 5, killTimeoutMs: 5,
    naturalExitMs: 5,
  });
  fixture.emit("Test Suite 'Selected tests' failed");
  await assert.rejects(result, (error) => {
    assert(error instanceof AggregateError);
    assert.equal(error.cause, error.errors[0]);
    assert.equal(error.errors[0].code, 'IOS_SELECTED_SUITE_FAILED');
    assert.match(error.errors[1].message, /did not close/);
    return true;
  });
});

test('failed marker cancels actor waits immediately while cancelAndJoin preserves natural xcresult flush', { timeout: 1000 }, async () => {
  const directory = mkdtempSync(join(tmpdir(), 'ios-suite-flush-'));
  const bundle = join(directory, 'flushed.xcresult');
  const fixture = watchedFixture();
  const actor = createIosSelectedSuiteActor({ watched: fixture.watched, selector: 'Actor/flush', timeoutMs: 1000,
    naturalExitMs: 500, closeTimeoutMs: 5, killTimeoutMs: 5,
    onPassed: () => assert.fail('Failed bundles must be retained'),
  });
  const actionRejected = assert.rejects(fixture.watched.waitForMarker('E2E: QR_GENERATED', 180_000),
    (error) => error.code === 'IOS_SELECTED_SUITE_FAILED');
  fixture.emit("Test Suite 'Selected tests' failed");
  try {
    await actionRejected;
    assert.deepEqual(fixture.events, ['cancel-pending']);
    const cancellation = actor.cancelAndJoin(); // The real registration cancellation handoff.
    assert.deepEqual(fixture.events, ['cancel-pending'], 'Sibling cleanup cannot kill a terminal suite during its flush window');
    writeFileSync(bundle, 'finalized QR attachment');
    fixture.finishNaturally(65);
    await cancellation;
    await assert.rejects(actor.result, (error) => error.code === 'IOS_SELECTED_SUITE_FAILED');
    assert.deepEqual(fixture.events, ['cancel-pending', 'natural-exit']);
    assert(existsSync(bundle));
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('passing suite exits naturally before cleanup without receiving a termination signal', async () => {
  const fixture = watchedFixture();
  let passed = false;
  const actor = createIosSelectedSuiteActor({ watched: fixture.watched, selector: 'Actor/natural-pass', timeoutMs: 1000,
    naturalExitMs: 500, onPassed: () => { assert.equal(fixture.watched.child.exitCode, 0); passed = true; },
  });
  fixture.emit("Test Suite 'Selected tests' passed");
  await Promise.resolve(); await Promise.resolve();
  fixture.finishNaturally();
  await actor.result;
  assert(passed);
  assert(!fixture.events.includes('SIGTERM'));
});

for (const termination of ['nonzero', 'signal']) {
  test(`passing suite cannot hide an unexpected natural ${termination} result-writer failure`, async () => {
    const fixture = watchedFixture();
    const failure = new Error(`xcodebuild result writer failed: ${termination}`);
    const actor = createIosSelectedSuiteActor({ watched: fixture.watched, selector: 'Actor/writer', timeoutMs: 1000,
      naturalExitMs: 500, onPassed: () => assert.fail('Unexpected process failure must prevent passed cleanup'),
    });
    fixture.emit("Test Suite 'Selected tests' passed");
    await Promise.resolve(); await Promise.resolve();
    if (termination === 'nonzero') fixture.failProcess(failure);
    else fixture.finishWithSignal(failure);
    await assert.rejects(actor.result, (error) => error === failure);
    assert(!fixture.events.includes('SIGTERM') && !fixture.events.includes('SIGKILL'));
  });
}

test('cancelAndJoin before a selected-suite result also owns bounded TERM/KILL cleanup', { timeout: 1000 }, async () => {
  const fixture = watchedFixture({ ignoreTerm: true });
  const actor = createIosSelectedSuiteActor({ watched: fixture.watched, selector: 'Actor/cancel', timeoutMs: 1000,
    naturalExitMs: 500, closeTimeoutMs: 5, killTimeoutMs: 50,
  });
  await actor.cancelAndJoin();
  await assert.rejects(actor.result, /cancelled before its selected-suite result/);
  assert.deepEqual(fixture.events, ['cancel-pending', 'cancel-pending', 'SIGTERM', 'SIGKILL']);
});

test('an early process failure remains the first cause and never invokes passed-result cleanup', async () => {
  const fixture = watchedFixture();
  const firstError = new Error('xcodebuild failed before a selected suite result');
  let cleanupCalls = 0;
  const result = superviseIosSelectedSuite({ watched: fixture.watched, selector: 'Actor/early', timeoutMs: 1000,
    onPassed: () => { cleanupCalls += 1; },
  });
  fixture.failProcess(firstError);
  await assert.rejects(result, (error) => error === firstError);
  assert.equal(cleanupCalls, 0);
  assert.deepEqual(fixture.events, ['cancel-pending']);
});
