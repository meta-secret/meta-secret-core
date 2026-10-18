import assert from 'node:assert/strict';
import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { registrationMarkers } from './plan.mjs';
import { runWebRegistrationActor, captureWebIncompleteIdentity, assertWebLocalIdentityCleared } from './web.mjs';

export async function runRegistrationCase({ scenario, page, startNativeActor, approvalCoordinator, artifactsDirectory, onEvent, cancelActors, proveIosLocalReset, runWebActor = runWebRegistrationActor }) {
  const entry = scenario.registrationCase;
  const runId = entry.runId;
  const ledger = registrationMarkers({ runId }, entry);
  const expected = new Set(ledger.map(({ key }) => key));
  const completed = new Set();
  const actors = [];
  const ignoredFailures = new Set();
  let rejectFirstFailure;
  let firstFailureError;
  const firstFailure = new Promise((_, reject) => { rejectFirstFailure = reject; });
  void firstFailure.catch(() => {});
  const guarded = (operation) => Promise.race([operation, firstFailure]);
  const gates = new Map();
  const markerPrefix = (platform) => `TEST51_${runId}_${entry.id}_${platform.toUpperCase()}`;
  const report = (key) => {
    if (!expected.has(key)) return; // Other fixture-creator actions are diagnostics.
    if (completed.has(key)) throw new Error(`Duplicate completed registration marker: ${key}`);
    completed.add(key); onEvent(key);
  };
  const screenshots = resolve(artifactsDirectory, runId, entry.id);
  mkdirSync(screenshots, { recursive: true });
  const actorConfig = (platform, actor, stage = 'primary', extra = {}) => ({
    ...entry, platform, actor, stage,
    provider: actor === 'requester' || entry.kind === 'create' ? entry.provider : 'manual',
    vaultName: stage === 'retry' ? entry.retryVaultName : entry.vaultName,
    markerPrefix: markerPrefix(platform), ...extra,
  });
  const start = async (platform, actor, stage = 'primary', extra = {}) => {
    const config = actorConfig(platform, actor, stage, extra);
    let running;
    if (platform !== 'web') running = await startNativeActor(platform, config);
    else {
      const webWaiters = new Map();
      const webMarkers = new Set();
      let rejectCancelled;
      const cancelled = new Promise((_, reject) => { rejectCancelled = reject; });
      const waitForMarker = (key, timeout = 180_000) => {
        if (webMarkers.has(key)) return Promise.resolve();
        return new Promise((resolveWait, reject) => {
          const timer = setTimeout(() => reject(new Error(`Registration marker deadline: ${key}`)), timeout);
          webWaiters.set(key, {
            resolve: () => { clearTimeout(timer); resolveWait(); },
            cancel: () => { clearTimeout(timer); reject(new Error(`Registration actor cancelled before ${key}`)); },
          });
        });
      };
      const ui = runWebActor({ page, actor: config,
        emit(action) {
          const key = `E2E: ${config.markerPrefix}_${action}`;
          webMarkers.add(key); webWaiters.get(key)?.resolve();
        },
        waitGate: () => new Promise((resolveGate) => gates.set(stage, resolveGate)),
        screenshot: (phase) => page.screenshot({ path: resolve(screenshots, `web-${stage}-${phase}.png`), fullPage: true }),
        // Read-only existing Core snapshot; never call reset/clear from tests.
        captureIncompleteIdentity: (name, kind) => captureWebIncompleteIdentity(page, name, kind),
        proveLocalReset: (attempt) => assertWebLocalIdentityCleared(page, attempt),
      });
      const result = Promise.race([ui, cancelled]);
      running = { result, waitForMarker, cancel() {
        for (const waiter of webWaiters.values()) waiter.cancel();
        rejectCancelled(new Error('Registration Web actor cancelled'));
      } };
    }
    void running.result.catch((error) => {
      if (ignoredFailures.has(running) || firstFailureError) return;
      firstFailureError = error;
      rejectFirstFailure(error);
    });
    actors.push(running);
    if (firstFailureError) throw firstFailureError;
    return running;
  };
  const wait = async (running, platform, action) => {
    const key = `E2E: ${markerPrefix(platform)}_${action}`;
    // Native test failure rejects before the marker's full deadline.
    const observed = running.waitForMarker(key, 180_000);
    await guarded(Promise.race([observed, running.result.then(() => observed)]));
    report(key);
  };
  const create = async (platform, stage = 'primary') => {
    const running = await start(platform, 'create', stage);
    const actions = entry.kind === 'create' ? ['READY', 'AVAILABLE', 'CREATED', 'QR_GENERATED'] : ['CREATED'];
    for (const action of actions) await wait(running, platform, action);
    await guarded(running.result);
  };
  const allow = (stage) => {
    approvalCoordinator.allow(`registration-${entry.id}-${stage}`, 0);
    gates.get(stage)?.();
  };
  try {
    report(`E2E: ${markerPrefix('harness')}_CLEAN`);
    if (entry.kind === 'create') {
      await create(entry.platform);
    } else {
      assert.equal(entry.kind, 'join', 'Physical QR scanner cases require their explicit manual handoff');
      await create(entry.approver);
      if (entry.outcome !== 'accepted') await create(entry.retryApprover, 'retry');
      // Back cases intentionally leave the first requester pending without a
      // responder action. Their first approver stays online after creation.
      const primary = await start(entry.approver, 'responder', 'primary', {
        decision: entry.outcome === 'declined-retry' ? 'declined' : 'accepted',
      });
      await wait(primary, entry.approver, 'READY');
      let retry;
      if (entry.outcome !== 'accepted') {
        retry = await start(entry.retryApprover, 'responder', 'retry', { decision: 'accepted' });
        await wait(retry, entry.retryApprover, 'READY');
      }
      const requester = await start(entry.platform, 'requester');
      for (const action of ['READY', 'TAKEN', 'PENDING']) await wait(requester, entry.platform, action);
      if (entry.platform === 'ios' && entry.outcome !== 'accepted') {
        assert(proveIosLocalReset, 'Scoped iOS identity cleanup oracle is required for retry');
        await guarded(proveIosLocalReset({ expectPresent: true }));
        approvalCoordinator.allow(`registration-${entry.id}-cleanup-presence-verified`, 0);
      }
      if (entry.outcome !== 'back-retry') {
        allow('primary');
        await wait(primary, entry.approver, 'DECIDED');
        await guarded(primary.result);
      }
      if (entry.outcome === 'declined-retry') await wait(requester, entry.platform, 'DECLINED');
      if (entry.outcome !== 'accepted') {
        if (entry.platform === 'ios' && entry.outcome !== 'accepted') {
          await wait(requester, 'ios', 'RESET_UI01');
          assert(proveIosLocalReset, 'Scoped iOS identity cleanup oracle is required before RESET01');
          await guarded(proveIosLocalReset({ expectPresent: false }));
          approvalCoordinator.allow(`registration-${entry.id}-reset-verified`, 0);
        }
        await wait(requester, entry.platform, 'RESET01');
        await wait(requester, entry.platform, 'RETRY_PENDING');
        allow('retry');
        await wait(retry, entry.retryApprover, 'DECIDED');
        await guarded(retry.result);
      }
      await wait(requester, entry.platform, 'MAIN');
      await guarded(requester.result);
      if (entry.outcome === 'back-retry') {
        ignoredFailures.add(primary);
        primary.cancel?.();
        await cancelActors([primary]);
      }
    }
    assert.equal(completed.size, expected.size, 'Every declared registration postcondition must complete');
    return { status: 'passed', id: entry.id, markers: completed.size };
  } catch (error) {
    for (const actor of actors) actor.cancel?.();
    await cancelActors(actors);
    throw error;
  }
}
