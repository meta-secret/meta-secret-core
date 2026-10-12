import assert from 'node:assert/strict';
import { parseCliEnvelope } from './cli-json.mjs';
import { waitForCanonicalTerminal, bindWebFailureGuard, browserOwnedSecretUsable } from './web-postcondition.mjs';

/** Test4 owns every process, readiness gate, diagnostic and cleanup resource. */
export async function runDeletionBlock(h) {
  let fail;
  const failure = new Promise((_, reject) => { fail = reject; });
  h.onNativeActor = (actor) => { void actor.result.catch((error) => { if (!actor.intentionalOfflineBoundary) fail(error); }); };
  const detachWebGuard = bindWebFailureGuard(h.page, fail);
  // The first child suite failure rejects the whole business run immediately.
  try { return await Promise.race([runBusiness(h), failure]); } finally { detachWebGuard(); }
}

async function runBusiness(h) {
  const { scenario, page, block, approvalCoordinator: coordinator } = h;
  const actors = new Set();
  let cliDevice;
  const scopeStep = (step) => `${block.runId}-${step}`;
  const marker = (platform, action, cycle, step) => `E2E: ${platform.toUpperCase()}_DELETION_${action}_${cycle}_${scopeStep(step)}`;
  const guarded = async (work) => Promise.race([
    work,
    ...[...actors].map((actor) => actor.result.then(() => new Promise(() => {}))),
  ]);
  const release = (platform, kind, cycle, step) => coordinator.allow(`${platform}-deletion-${kind}-${scopeStep(step)}`, cycle.number);
  const native = async (platform, role, cycle, step, options = {}) => {
    const environment = {
      E2E_ROLE: role, E2E_CYCLE: String(cycle.number), E2E_STEP: scopeStep(step),
      E2E_EXPECTED_OUTCOME: cycle.outcome, E2E_EXPECT_REQUEST: String(block.members.length === 3),
      ...options.environment,
    };
    const actor = platform === 'android' ? await h.startAndroidStepTest(h.androidSerial, {
      role, cycle: cycle.number, step: scopeStep(step), secretName: cycle.secretName, secretValue: h.secretValue(cycle.secretName),
      expectedOutcome: cycle.outcome, expectRequest: block.members.length === 3,
      testClass: scenario.android.testClass, testMethod: 'handleSecretDeletionStep', ...options,
    }) : h.startIosJoinTest(h.simulatorUdid, {
      testClass: scenario.ios.testClass, testMethod: 'handleSecretDeletionStep',
      secretName: cycle.secretName, secretValue: h.secretValue(cycle.secretName), environment,
    });
    actors.add(actor); h.onNativeActor(actor);
    // Attach immediately, but propagate the first rejection through guarded barriers.
    void actor.result.catch(() => {});
    await guarded(actor.waitForMarker(marker(platform, 'READY', cycle.number, step), 180_000));
    if (platform === 'android' && step === 'recreated-show-android') actor.observedSecretId = await guarded(actor.waitForSecretId(`ANDROID_DELETION_ID_${cycle.number}_${scopeStep(step)}`, 180_000));
    return actor;
  };
  const wait = (actor, platform, action, cycle, step) => guarded(actor.waitForMarker(marker(platform, action, cycle.number, step), 180_000));
  const finish = async (actor) => { await guarded(actor.result); actors.delete(actor); };
  const cli = async (args, { input, allowFailure = false } = {}) => {
    const result = await h.runCli(cliDevice.cliDirectory, args, { input, allowFailure });
    return result;
  };
  const json = parseCliEnvelope;
  const readCoreState = async () => {
    if (block.members.includes('web')) return page.evaluate(async () => {
      const { AppState } = await import('/src/stores/app-state.ts');
      const store = AppState();
      const member = store.currState.as_vault().as_member();
      return { states: JSON.parse(member.deletion_states_json()), secrets: member.vault_data().secrets().map((secret) => { window.__TEST50_SECRET_OBJECTS ??= {}; window.__TEST50_SECRET_OBJECTS[secret.id_str()] = secret; return { name: secret.name, id: secret.id_str() }; }) };
    });
    if (cliDevice) {
      const requests = json((await cli(['secret', 'deletion-requests'])).stdout);
      const secrets = json((await cli(['info', 'secrets'])).stdout);
      return { states: requests, secrets: secrets.secrets ?? [] };
    }
    return h.nativeCanonicalMetadata(block.members[0]); // Observe existing sync response; never issue a passive RPC.
  };
  const coreState = async () => {
    const state = await readCoreState();
    state.secrets.sort((a, b) => a.id.localeCompare(b.id));
    state.states.sort((a, b) => `${a.requestId ?? a.request_id ?? ''}/${a.secretId ?? a.secret_id ?? ''}`.localeCompare(`${b.requestId ?? b.request_id ?? ''}/${b.secretId ?? b.secret_id ?? ''}`));
    state.deletedIds?.sort();
    return state;
  };
  const normalizedState = (state) => ({
    id: state.secretId ?? state.secret_id,
    requestId: state.requestId ?? state.request_id,
    status: state.clientStatus ?? state.client_status,
  });
  const secretId = async (name) => (await coreState())?.secrets.find((secret) => secret.name === name)?.id;
  const webRow = (name) => page.getByTestId(`secret-primary-action-${name}`);
  const assertWebTerminal = async (cycle, { allowExistingSyncError = false } = {}) => {
    await page.evaluate(async () => { const { AppState } = await import('/src/stores/app-state.ts'); window.__TEST50_STORE = AppState(); });
    const terminal = await page.evaluate(waitForCanonicalTerminal, {
      passId: cycle.passId, requestId: cycle.requestId,
      expectedStatus: cycle.outcome === 'accepted' ? 'Done' : 'Declined', allowExistingSyncError,
    });
    cycle.requestId = terminal.requestId;
    await webRow(cycle.secretName).waitFor({ state: cycle.outcome === 'accepted' ? 'hidden' : 'visible', timeout: 120_000 });
    await page.getByTestId('approve-secret-deletion').waitFor({ state: 'hidden', timeout: 120_000 });
    if (cycle.outcome === 'declined') {
      await page.getByTestId(`delete-secret-${cycle.secretName}`).waitFor({ state: 'visible', timeout: 120_000 });
      assert.equal(await page.getByTestId(`delete-secret-${cycle.secretName}`).isEnabled(), true);
    }
  };
  const create = async (cycle, generation = 'create') => {
    const creator = block.members[0];
    if (creator === 'web') return h.createWebSecret(page, cycle.secretName);
    if (creator === 'cli') {
      await cli(['secret', 'split', '--pass-name', cycle.secretName, '--stdin'], { input: `${h.secretValue(cycle.secretName)}\n` });
      return;
    }
    const actor = creator === 'android' ? await h.startAndroidStepTest(h.androidSerial, {
      role: 'create', cycle: cycle.number, step: scopeStep(generation), secretName: cycle.secretName,
      secretValue: h.secretValue(cycle.secretName), testClass: scenario.android.testClass,
      testMethod: 'createSecretForTest50',
    }) : h.startIosJoinTest(h.simulatorUdid, {
      testClass: scenario.ios.testClass, testMethod: 'createSecretForTest50',
      secretName: cycle.secretName, secretValue: h.secretValue(cycle.secretName),
      environment: { E2E_ROLE: 'create', E2E_CYCLE: String(cycle.number), E2E_STEP: scopeStep(generation) },
    });
    await actor.waitForMarker(`E2E: ${creator.toUpperCase()}_TEST50_SECRET_CREATED_${cycle.number}_${scopeStep(generation)}`, 180_000);
    await actor.result;
  };

  // F3/build/installation are complete before this setup makes its first mutation.
  const initiator = block.members[0];
  h.setJoinExpectedSecretNames([scenario.secret.name]);
  if (initiator === 'web') await h.createWebInitiatedVault(page);
  else if (initiator === 'cli') cliDevice = await h.createMetaCliRootDevice();
  else {
    const actor = initiator === 'android' ? await h.startAndroidJoinTest(h.androidSerial, {
      testClass: scenario.android.testClass, testMethod: 'createVaultForTest50',
    }) : h.startIosJoinTest(h.simulatorUdid, { testClass: scenario.ios.testClass, testMethod: 'createVaultForTest50', secretName: scenario.secret.name, secretValue: scenario.secret.value, environment: { E2E_ROLE: 'setup', E2E_CYCLE: '0', E2E_STEP: 'setup' } });
    await actor.waitForMarker(`E2E: ${initiator.toUpperCase()}_TEST50_INITIATOR_SETUP_READY`, 180_000);
    await actor.result;
  }
  for (const joiner of block.members.slice(1)) {
    if (joiner === 'web') { await h.joinWebToCliVault(page, cliDevice, 'web'); await h.waitForWebSecretNames(page, [scenario.secret.name]); continue; }
    const actor = joiner === 'android' ? await h.startAndroidJoinTest(h.androidSerial, {
      testClass: scenario.android.testClass, testMethod: 'joinWebInitiatedVault', expectedSecretNames: [scenario.secret.name],
    }) : h.startIosJoinTest(h.simulatorUdid, { testClass: scenario.ios.testClass, testMethod: 'joinVaultForTest50', environment: { E2E_ROLE: 'join', E2E_CYCLE: '0', E2E_STEP: 'join' } });
    await actor.waitForMarker(`E2E: ${joiner.toUpperCase()}_JOIN_REQUEST_SENT`, 180_000);
    if (initiator === 'web') await h.approveJoinRequestOnWeb(page, joiner === 'ios' ? 'iOS' : 'Android');
    else await h.acceptNextCliJoin(cliDevice, joiner);
    await actor.waitForMarker(`E2E: ${joiner.toUpperCase()}_JOIN_READY`, 180_000);
    await actor.result;
  }
  if (block.members.includes('web')) await h.waitForWebSecretNames(page, [scenario.secret.name]);
  // Exact canonical names are opaque; helpers assert unambiguous member rows.
  if (block.members.includes('web')) await h.assertWebMembership(page,
    block.members.map((platform) => ({ web: 'Web', android: 'Android', ios: 'iPhone' }[platform])),
    [scenario.secret.name], 'Test50 canonical identities');

  for (const cycle of block.cycles) {
    h.onCycle(cycle);
    await create(cycle);
    if (block.members.includes('web')) await h.waitForWebSecretNames(page, [cycle.secretName]);
    const oldId = await secretId(cycle.secretName);
    assert(oldId, 'A current canonical opaque ID is required before any deletion action');
    cycle.passId = oldId;
    const requester = cycle.requester;
    // Local cancellation must preserve identity and not create a request.
    const beforeCancel = await coreState();
    if (requester === 'web') {
      await page.getByTestId(`delete-secret-${cycle.secretName}`).click();
      await page.getByRole('button', { name: 'Cancel', exact: true }).click();
      await page.getByTestId('confirm-delete-secret').waitFor({ state: 'hidden' });
    } else if (requester === 'cli') {
      const refused = await cli(['secret', 'delete', '--pass-name', cycle.secretName], { allowFailure: true });
      assert.notEqual(refused.exitCode, 0, 'Noninteractive unconfirmed CLI deletion must refuse');
    } else {
      const actor = await native(requester, 'cancel', cycle, 'cancel');
      release(requester, 'action', cycle, 'cancel');
      await wait(actor, requester, 'CANCELLED', cycle, 'cancel'); await finish(actor);
    }
    assert.deepEqual(await coreState(), beforeCancel, 'Cancel must leave canonical state unchanged');

    // At 1/2 members all local Show actors test real reveal and its own refresh.
    if (block.members.length <= 2) for (const platform of block.members) {
      if (platform === 'cli') {
        const shown = await cli(['secret', 'show-local', '--pass-name', cycle.secretName]);
        assert.equal(shown.stdout.includes(h.secretValue(cycle.secretName)), true, 'CLI exact local value');
      } else if (platform === 'web') await h.showAndCloseWebSecret(page, cycle.secretName);
      else {
        const actor = await native(platform, 'show', cycle, `show-${platform}`);
        release(platform, 'action', cycle, `show-${platform}`); release(platform, 'terminal', cycle, `show-${platform}`);
        await wait(actor, platform, 'SHOW_VALID', cycle, `show-${platform}`); await finish(actor);
      }
    }

    const mainActors = new Map();
    const coldOffline = ['android', 'ios'].includes(cycle.offline);
    // All receiver setup precedes the request. CLI is active only when commanded.
    for (const platform of block.members.filter((platform) => platform !== requester)) {
      if (['android', 'ios'].includes(platform)) mainActors.set(platform, await native(platform,
        platform === cycle.offline ? 'sync-observe' : platform === cycle.responder ? (cycle.outcome === 'accepted' ? 'approve' : 'decline') : 'observe',
        cycle, 'main'));
      else if (platform === 'web') await webRow(cycle.secretName).waitFor({ state: 'visible' });
      else await cli(['info', 'secrets']);
    }
    if (cycle.offline && ['android', 'ios'].includes(cycle.offline)) {
      await h.setNetworkGate(cycle.offline, false);
      release(cycle.offline, 'action', cycle, 'main');
      await wait(mainActors.get(cycle.offline), cycle.offline, 'SYNC_BLOCKED', cycle, 'main');
      if (coldOffline) {
        const offlineActor = mainActors.get(cycle.offline);
        offlineActor.intentionalOfflineBoundary = true;
        mainActors.delete(cycle.offline); actors.delete(offlineActor);
        await h.cancelOfflineActor(cycle.offline, offlineActor);
      }
    } else if (cycle.offline === 'web') {
      await page.evaluate(async () => {
        const { AppState } = await import('/src/stores/app-state.ts');
        const manager = AppState().appManager;
        const original = manager.show_local_secret.bind(manager);
        manager.show_local_secret = async (secret) => {
          const value = await original(secret);
          window.__TEST50_LATE_REVEAL_READY = true;
          return new Promise((resolve) => { window.__TEST50_RELEASE_LATE_REVEAL = () => { manager.show_local_secret = original; resolve(value); }; });
        };
      });
      await webRow(cycle.secretName).click();
      await page.waitForFunction(() => window.__TEST50_LATE_REVEAL_READY === true, null, { timeout: 120_000 });
      await page.context().setOffline(true);
    }
    // An offline CLI makes no commands until the reconnect boundary.
    let requestId;
    if (requester === 'web') {
      await page.evaluate(async ({ name, id }) => {
        const { AppState } = await import('/src/stores/app-state.ts');
        const owned = AppState().passwords.find((secret) => secret.id_str() === id);
        if (!owned || owned.name !== name) throw new Error('Web current Secret binding did not match the exact identity');
        window.__TEST50_OWNED_DELETE_SECRET = owned;
      }, { name: cycle.secretName, id: oldId });
      await page.getByTestId(`delete-secret-${cycle.secretName}`).click();
      await page.getByTestId('confirm-delete-secret').click();
    } else if (requester === 'cli') {
      const result = normalizedState(json((await cli(['secret', 'delete', '--pass-name', cycle.secretName, '--yes'])).stdout));
      requestId = result.requestId;
      assert.equal(result.status, block.members.length === 3 ? 'Pending' : 'Done');
    } else {
      const actor = await native(requester, 'requester', cycle, 'main'); mainActors.set(requester, actor);
      release(requester, 'action', cycle, 'main');
      await wait(actor, requester, 'SUBMITTED', cycle, 'main');
      if (block.members.length <= 2) {
        const terminal = h.nativeCanonicalMetadata(requester).states.find((state) => state.secretId === oldId && state.clientStatus === 'Done');
        assert(terminal?.requestId, 'Native local deletion must have an exact persisted terminal UUID');
        requestId = terminal.requestId;
      }
    }
    if (block.members.length === 3) {
      // Two receivers independently observe exactly this pending request before any response.
      for (const platform of block.members.filter((platform) => platform !== requester)) {
        if (platform === 'web') {
          await page.getByTestId('approve-secret-deletion').waitFor({ state: 'visible', timeout: 120_000 });
          requestId = await page.getByTestId('approve-secret-deletion').getAttribute('data-request-id');
        } else if (platform === 'cli') {
          const pending = json((await cli(['secret', 'deletion-requests'])).stdout).map(normalizedState).find((state) => state.id === oldId && state.status === 'NeedApprove');
          assert(pending?.requestId, 'CLI must observe exact incoming request'); requestId = pending.requestId;
        } else await wait(mainActors.get(platform), platform, 'REQUEST_VISIBLE', cycle, 'main');
      }
      // Authoritative UUID/status snapshot is taken after rendered receiver boundaries.
      const states = (await coreState()).states.map(normalizedState);
      const pending = states.find((state) => state.id === oldId && ['Pending', 'NeedApprove'].includes(state.status));
      assert(pending?.requestId, 'Exact persisted pending UUID is required'); requestId = pending.requestId;
      const responder = cycle.responder;
      if (responder === 'web') await page.getByTestId(cycle.outcome === 'accepted' ? 'approve-secret-deletion' : 'decline-secret-deletion').click();
      else if (responder === 'cli') await cli(['secret', cycle.outcome === 'accepted' ? 'approve-deletion' : 'decline-deletion', '--request-id', requestId, ...(cycle.outcome === 'accepted' ? ['--yes'] : [])]);
      else {
        release(responder, 'action', cycle, 'main');
        await wait(mainActors.get(responder), responder, 'SUBMITTED', cycle, 'main');
      }
    }
    // Persisted terminal result, scoped to UUID and original Pass ID.
    cycle.requestId = requestId;
    if (block.members.includes('web') && cycle.offline !== 'web') await assertWebTerminal(cycle);
    if (requester === 'web') {
      const usable = await page.evaluate(browserOwnedSecretUsable, { name: cycle.secretName, id: oldId });
      assert.equal(usable, true, 'Awaited deletion must not consume the browser-owned Secret binding');
    }
    if (cliDevice && cycle.offline !== 'cli') {
      const terminal = (await coreState()).states.map(normalizedState).find((state) => state.requestId === requestId || (state.id === oldId && state.status === 'Done'));
      assert.equal(terminal?.status, cycle.outcome === 'accepted' ? 'Done' : 'Declined', 'CLI Core terminal');
    }
    if (cycle.offline) {
      if (['android', 'ios'].includes(cycle.offline)) {
        if (coldOffline) {
          h.holdNetworkGate(cycle.offline);
          const startup = await native(cycle.offline, 'startup-gone', cycle, 'startup');
          await h.waitForHeldNetworkRequest(cycle.offline);
          await h.setNetworkGate(cycle.offline, true);
          release(cycle.offline, 'action', cycle, 'startup'); release(cycle.offline, 'terminal', cycle, 'startup');
          await wait(startup, cycle.offline, 'TERMINAL', cycle, 'startup'); await finish(startup);
        } else { await h.setNetworkGate(cycle.offline, true); release(cycle.offline, 'sync-release', cycle, 'main'); }
      } else if (cycle.offline === 'web') {
        await page.context().setOffline(false);
        // Actual reconnect/SSE drives this terminal UI; no fixture updateState call.
        await assertWebTerminal(cycle, { allowExistingSyncError: true });
        await page.evaluate(() => window.__TEST50_RELEASE_LATE_REVEAL());
        await page.getByText(h.secretValue(cycle.secretName), { exact: true }).waitFor({ state: 'hidden', timeout: 30_000 });
        await page.reload(); await h.unlockWithPasskeyIfNeeded(page); await assertWebTerminal(cycle);
      }
      else {
        const terminal = await coreState();
        assert.equal(terminal.secrets.some((secret) => secret.name === cycle.secretName), false, 'Offline CLI cleanup before state return');
      }
    }
    for (const [platform, actor] of mainActors) {
      release(platform, 'terminal', cycle, 'main');
      await wait(actor, platform, 'TERMINAL', cycle, 'main'); await finish(actor);
    }
    const terminalState = (await coreState()).states.map(normalizedState).find((state) =>
      state.id === oldId && (requestId ? state.requestId === requestId : state.status === 'Done'));
    assert.equal(terminalState?.status, cycle.outcome === 'accepted' ? 'Done' : 'Declined', 'Exact persisted request terminal state');
    assert(terminalState.requestId, 'A terminal UUID is required');
    requestId = terminalState.requestId;
    h.onDecisionPersisted(cycle, requestId);
    // Late opposite CLI decisions use the SAME UUID, never a new request.
    if (cliDevice && requestId && block.members.length === 3 && requester !== 'cli') {
      const before = await coreState();
      await cli(['secret', cycle.outcome === 'accepted' ? 'decline-deletion' : 'approve-deletion', '--request-id', requestId, ...(cycle.outcome === 'declined' ? ['--yes'] : [])], { allowFailure: true });
      assert.deepEqual(await coreState(), before, 'First terminal response wins');
    }
    if (cycle.outcome === 'accepted') {
      h.setSecretValue(cycle.secretName, `${h.secretValue(cycle.secretName)}-recreated`);
      await create(cycle, 'recreate'); // SAME display name, Core generates the fresh opaque ID.
      if (block.members.includes('web')) await h.waitForWebSecretNames(page, [cycle.secretName]);
      const newId = await secretId(cycle.secretName);
      assert(oldId && newId && newId !== oldId, 'Same name must have a fresh canonical ID');
      if (cliDevice && oldId) {
        const stale = await cli(['secret', 'show-local', '--pass-name', oldId], { allowFailure: true });
        assert.notEqual(stale.exitCode, 0, 'Recreated display name must not make the old opaque ID revealable');
      }
      if (block.members.includes('web') && oldId) {
        const refused = await page.evaluate(async (id) => {
          const { AppState } = await import('/src/stores/app-state.ts');
          try { await AppState().appManager.show_local_secret(window.__TEST50_SECRET_OBJECTS[id]); return false; } catch { return true; }
        }, oldId);
        assert.equal(refused, true, 'WASM must reject the old opaque ID after same-name recreation');
      }
      if (block.members.length <= 2) {
        if (initiator === 'web') await h.showAndCloseWebSecret(page, cycle.secretName);
        else if (initiator === 'cli') { const shown = await cli(['secret', 'show-local', '--pass-name', cycle.secretName]); assert.equal(shown.stdout.includes(h.secretValue(cycle.secretName)), true, 'Recreated CLI value'); }
        else {
          const actor = await native(initiator, 'show', cycle, `recreated-show-${initiator}`);
          release(initiator, 'action', cycle, `recreated-show-${initiator}`); release(initiator, 'terminal', cycle, `recreated-show-${initiator}`);
          await wait(actor, initiator, 'SHOW_VALID', cycle, `recreated-show-${initiator}`); await finish(actor);
          if (initiator === 'android' && oldId) { const observed = actor.observedSecretId; assert(observed && observed !== oldId && observed === newId, 'Native recreation must bind the exact fresh canonical ID'); }
        }
      }
      // Current row/controls bind the newly created Secret, never prior terminal history.
      if (block.members.includes('web')) assert.equal(await page.getByTestId(`delete-secret-${cycle.secretName}`).isEnabled(), true);
    }
    h.onCycleComplete(cycle);
  }
  assert.equal(actors.size, 0, 'Every native actor must reach its terminal suite result');
}
