import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildRegistrationPlan, registrationMarkers } from './plan.mjs';
import { runRegistrationCase } from './business.mjs';

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
};

test('Web fixture READY cannot complete or duplicate responder READY; actors stay online through approval', async () => {
  const plan = buildRegistrationPlan('flow');
  const entry = { ...plan.cases.find(({ id }) => id === 'join-android-manual-accepted'), runId: plan.runId };
  const artifactDirectory = mkdtempSync(join(tmpdir(), 'registration-f1-'));
  const events = [];
  const requesterDecision = deferred();
  let readyActors = 0;
  const runWebActor = async ({ actor, emit, waitGate }) => {
    if (actor.actor === 'create') {
      for (const marker of ['READY', 'AVAILABLE', 'CREATED', 'QR_GENERATED']) emit(marker);
    } else {
      assert.equal(actor.actor, 'responder');
      readyActors += 1;
      emit('READY');
      await waitGate();
      emit('DECIDED');
      requesterDecision.resolve();
    }
  };
  const startNativeActor = async (platform, actor) => {
    assert.equal(platform, 'android');
    assert.equal(actor.actor, 'requester');
    assert.equal(readyActors, 1, 'Responder readiness precedes the request mutation');
    const markers = new Set();
    const waiters = new Map();
    const emit = (action) => {
      const key = `E2E: ${actor.markerPrefix}_${action}`;
      markers.add(key); waiters.get(key)?.resolve();
    };
    const result = (async () => {
      for (const marker of ['READY', 'TAKEN', 'PENDING']) emit(marker);
      await requesterDecision.promise;
      emit('MAIN');
    })();
    return { result, waitForMarker(key) {
      if (markers.has(key)) return Promise.resolve();
      const pending = deferred(); waiters.set(key, pending); return pending.promise;
    } };
  };
  try {
    const result = await runRegistrationCase({ scenario: { registrationCase: entry }, page: {},
      startNativeActor, runWebActor, approvalCoordinator: { allow() {} }, artifactsDirectory: artifactDirectory,
      onEvent: (event) => events.push(event), cancelActors: async () => assert.fail('No actor must be cancelled on accepted Join'),
    });
    assert.equal(result.status, 'passed');
    assert.equal(events.length, registrationMarkers(plan, entry).length);
    assert.equal(events.filter((event) => event.endsWith('_WEB_READY')).length, 1);
    assert(events.indexOf(`E2E: TEST51_flow_${entry.id}_WEB_READY`) < events.indexOf(`E2E: TEST51_flow_${entry.id}_ANDROID_PENDING`));
  } finally { rmSync(artifactDirectory, { recursive: true, force: true }); }
});

test('decisive native failure cancels the ready Web observer instead of accumulating marker deadlines', async () => {
  const plan = buildRegistrationPlan('cancel');
  const entry = { ...plan.cases.find(({ id }) => id === 'join-android-manual-accepted'), runId: plan.runId };
  const artifactDirectory = mkdtempSync(join(tmpdir(), 'registration-f1-'));
  let cancelled = 0;
  try {
    await assert.rejects(runRegistrationCase({ scenario: { registrationCase: entry }, page: {},
      async runWebActor({ actor, emit, waitGate }) {
        if (actor.actor === 'create') for (const action of ['READY', 'AVAILABLE', 'CREATED', 'QR_GENERATED']) emit(action);
        else { emit('READY'); await waitGate(); }
      },
      async startNativeActor() {
        return { result: Promise.reject(new Error('Canonical production failure observed')), waitForMarker: () => new Promise(() => {}) };
      }, approvalCoordinator: { allow() {} }, artifactsDirectory: artifactDirectory,
      onEvent() {}, cancelActors: async (actors) => { cancelled = actors.length; },
    }), /Canonical production failure observed/);
    assert.equal(cancelled, 3, 'Fixture creator, responder and requester belong to the same cancellation barrier');
  } finally { rmSync(artifactDirectory, { recursive: true, force: true }); }
});

test('a responder failure after READY interrupts a different actor pending wait', { timeout: 1_000 }, async () => {
  const plan = buildRegistrationPlan('sibling');
  const entry = { ...plan.cases.find(({ id }) => id === 'join-android-manual-accepted'), runId: plan.runId };
  const artifactDirectory = mkdtempSync(join(tmpdir(), 'registration-f1-'));
  const failResponder = deferred();
  let cancelled = false;
  try {
    await assert.rejects(runRegistrationCase({ scenario: { registrationCase: entry }, page: {},
      async runWebActor({ actor, emit }) {
        if (actor.actor === 'create') for (const action of ['READY', 'AVAILABLE', 'CREATED', 'QR_GENERATED']) emit(action);
        else { emit('READY'); await failResponder.promise; throw new Error('Responder died after READY'); }
      },
      async startNativeActor(_platform, actor) {
        const available = new Set(['READY', 'TAKEN'].map((action) => `E2E: ${actor.markerPrefix}_${action}`));
        failResponder.resolve();
        return { result: new Promise(() => {}), waitForMarker: (key) => available.has(key) ? Promise.resolve() : new Promise(() => {}) };
      }, approvalCoordinator: { allow() {} }, artifactsDirectory: artifactDirectory,
      onEvent() {}, cancelActors: async () => { cancelled = true; },
    }), /Responder died after READY/);
    assert(cancelled, 'First sibling failure must invoke the common cancellation barrier');
  } finally { rmSync(artifactDirectory, { recursive: true, force: true }); }
});


for (const outcome of ['back-retry', 'declined-retry']) {
  test(`${outcome} iOS requires positive host proof before Back and zero proof before RESET01`, { timeout: 1_000 }, async () => {
    const plan = buildRegistrationPlan('cleanup');
    const entry = { ...plan.cases.find(({ id }) => id === `join-ios-manual-${outcome}`), runId: plan.runId };
    const artifactDirectory = mkdtempSync(join(tmpdir(), 'registration-f1-'));
    const events = [], ordering = [], allowed = [];
    const retryDecision = deferred(), primaryDecision = deferred(), primaryStopped = deferred();
    const presenceGate = deferred(), resetGate = deferred();
    const positiveEntered = deferred(), positiveRelease = deferred();
    let primary, backStarted = false, respondersReady = 0;
    const native = (actor, perform) => {
      const markers = new Set(), waiters = new Map();
      const emit = (action) => {
        const key = `E2E: ${actor.markerPrefix}_${action}`;
        markers.add(key); waiters.get(key)?.resolve();
      };
      return { result: perform(emit), waitForMarker(key) {
        if (markers.has(key)) return Promise.resolve();
        const pending = deferred(); waiters.set(key, pending); return pending.promise;
      } };
    };
    try {
      const running = runRegistrationCase({ scenario: { registrationCase: entry }, page: {},
        async runWebActor({ actor, emit, waitGate }) {
          assert.equal(actor.stage, 'retry');
          if (actor.actor === 'create') for (const action of ['READY', 'AVAILABLE', 'CREATED', 'QR_GENERATED']) emit(action);
          else { respondersReady += 1; emit('READY'); await waitGate(); emit('DECIDED'); retryDecision.resolve(); }
        },
        async startNativeActor(platform, actor) {
          if (platform === 'android') {
            if (actor.actor === 'create') return native(actor, async (emit) => emit('CREATED'));
            primary = native(actor, async (emit) => {
              respondersReady += 1; emit('READY');
              if (outcome === 'back-retry') await primaryStopped.promise;
              else { await primaryDecision.promise; emit('DECIDED'); }
            });
            return primary;
          }
          assert.equal(platform, 'ios'); assert.equal(actor.actor, 'requester');
          assert.equal(respondersReady, 2);
          return native(actor, async (emit) => {
            for (const action of ['READY', 'TAKEN', 'PENDING']) emit(action);
            if (outcome === 'declined-retry') { await primaryDecision.promise; emit('DECLINED'); }
            await presenceGate.promise;
            backStarted = true; ordering.push('back'); emit('RESET_UI01');
            await resetGate.promise; ordering.push('reset01'); emit('RESET01'); emit('RETRY_PENDING');
            await retryDecision.promise; emit('MAIN');
          });
        },
        approvalCoordinator: { allow(name) {
          allowed.push(name);
          if (name === `registration-${entry.id}-cleanup-presence-verified`) { ordering.push('presence-gate'); presenceGate.resolve(); }
          else if (name === `registration-${entry.id}-reset-verified`) { ordering.push('reset-gate'); resetGate.resolve(); }
          else if (name === `registration-${entry.id}-primary`) { assert.equal(outcome, 'declined-retry'); primaryDecision.resolve(); }
          else assert.equal(name, `registration-${entry.id}-retry`);
        } },
        async proveIosLocalReset({ expectPresent }) {
          assert.equal(backStarted, !expectPresent, 'Positive host COUNT must precede Back; zero COUNT must follow it');
          if (expectPresent) { positiveEntered.resolve(); await positiveRelease.promise; }
          ordering.push(expectPresent ? 'positive-count' : 'zero-count');
        },
        artifactsDirectory: artifactDirectory, onEvent: (event) => events.push(event),
        async cancelActors(actors) { assert.equal(outcome, 'back-retry'); assert.deepEqual(actors, [primary]); primaryStopped.resolve(); },
      });
      await positiveEntered.promise;
      assert.equal(backStarted, false, 'A held positive oracle must prevent Back');
      assert.deepEqual(allowed, [], 'Neither cleanup nor primary approval may open before positive proof');
      positiveRelease.resolve();
      const result = await running;
      assert.equal(result.status, 'passed');
      assert.equal(result.markers, registrationMarkers(plan, entry).length);
      assert.deepEqual(ordering, ['positive-count', 'presence-gate', 'back', 'zero-count', 'reset-gate', 'reset01']);
      assert(events.some((event) => event.endsWith('_IOS_RESET01')));
      assert(events.some((event) => event.endsWith('_IOS_RESET_UI01')));
      assert(!events.some((event) => event.endsWith('_IOS_BACK02')));
      assert.equal(allowed.includes(`registration-${entry.id}-primary`), outcome === 'declined-retry');
    } finally { positiveRelease.resolve(); primaryStopped.resolve(); rmSync(artifactDirectory, { recursive: true, force: true }); }
  });
}
