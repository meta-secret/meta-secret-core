export function recoveryClaimIds(claimsInfo, password) {
  return new Set((claimsInfo.claims ?? [])
    .filter((claim) => claim.type === 'Recover' && claim.password === password)
    .map((claim) => claim.id));
}

export function selectNewPendingRecoveryClaim(claimsInfo, {
  password,
  baselineClaimIds,
  receiverId,
  senderId,
}) {
  const candidates = (claimsInfo.claims ?? []).filter((claim) => (
    claim.type === 'Recover'
      && claim.password === password
      && !baselineClaimIds.has(claim.id)
      && (!senderId || claim.sender === senderId)
      && (!receiverId || (claim.receivers ?? []).some((receiver) => (
        receiver.id === receiverId && receiver.status === 'Pending'
      )))
  ));
  if (candidates.length !== 1) return null;
  return candidates[0];
}

export function test15FifoContext(group) {
  const fail = () => { throw new Error('Test15 FIFO scope/approval/role identity mismatch'); };
  const scope = group?.scope;
  if (!scope || typeof scope.runId !== 'string' || !scope.runId || Object.keys(scope).length !== 5 || scope.testNumber !== 15 || scope.cycle !== 1 || scope.step !== 1
    || !['scenario-block1', 'scenario-block2', 'scenario-block3'].includes(scope.block)) fail();
  if (scope.block !== 'scenario-block1') {
    const expected = scope.block === 'scenario-block2' ? ['ios', 'android'] : ['android', 'ios'];
    if (group.sender !== expected[0] || group.removalApprover !== expected[0]
      || group.removalTarget !== expected[1] || group.approvals?.length !== 0) fail();
    return null;
  }
  if (group.sender !== 'web' || group.removalApprover !== 'android' || group.removalTarget !== 'ios'
    || !group.vaultId || !group.secret?.passId || !group.secret.name || group.approvals?.length !== 1) fail();
  const approval = group.approvals[0];
  if (approval.platform !== 'android' || approval.action !== 'decline' || approval.secret !== group.secret.name) fail();
  const devices = ['web', 'android', 'ios'].map((role) => group.devices?.[role]);
  if (devices.some((device) => !device?.id || !device.name) || new Set(devices.map((device) => device.id)).size !== 3
    || !Array.isArray(group.baselineRequestIds) || new Set(group.baselineRequestIds).size !== group.baselineRequestIds.length) fail();
  const result = JSON.parse(JSON.stringify(group));
  const freeze = (value) => { if (value && typeof value === 'object') { Object.values(value).forEach(freeze); Object.freeze(value); } return value; };
  return freeze(result);
}

export function assertTest15FifoTransition({ context: c, phase, recoveryId, removalId }, projection) {
  const fail = (reason) => { throw new Error(`Test15 canonical ${phase}: ${reason}`); };
  if (!['recover-pending', 'both-pending', 'recover-declined', 'removed'].includes(phase)) fail('unknown phase');
  if (!projection?.session?.member || !projection.session.ready || projection.session.vaultId !== c.vaultId) fail('Vault identity/member not ready');
  const expectedRoles = phase === 'removed' ? ['web', 'android'] : ['web', 'android', 'ios'];
  if (projection.devices?.length !== expectedRoles.length) fail('membership count changed');
  for (const role of expectedRoles) {
    const matches = projection.devices.filter((device) => device.deviceId === c.devices[role].id);
    if (matches.length !== 1 || matches[0].name !== c.devices[role].name || Boolean(matches[0].current) !== (role === 'web')) fail('device identity changed');
  }
  const passes = projection.secrets?.filter((secret) => secret.passId === c.secret.passId || secret.name === c.secret.name) ?? [];
  if (passes.length !== 1 || passes[0].passId !== c.secret.passId || passes[0].name !== c.secret.name) fail('Pass identity changed');
  const operations = projection.operations ?? [], baseline = new Set(c.baselineRequestIds);
  if (operations.some((op) => ['recover', 'removeDevice'].includes(op.kind) && baseline.has(op.requestId)
    && ['pending', 'needApprove'].includes(op.clientStatus) && op.outcome === undefined)) fail('stale pending recovery/removal crosses the clean boundary');
  const recoveries = operations.filter((op) => op.kind === 'recover' && !baseline.has(op.requestId));
  if (!recoveries.length && phase === 'recover-pending' && !recoveryId) return null;
  if (recoveries.length !== 1) fail('recovery Claim must be unique and new');
  const recovery = recoveries[0];
  if (!recovery.requestId || recoveryId && recovery.requestId !== recoveryId || baseline.has(recovery.requestId)
    || recovery.requesterId !== c.devices.web.id || recovery.secretId !== c.secret.passId) fail('recovery identity changed');
  const pending = (op) => ['pending', 'needApprove'].includes(op.clientStatus) && op.outcome === undefined;
  if (['recover-pending', 'both-pending'].includes(phase)) {
    if (!pending(recovery) || JSON.stringify([...recovery.eligiblePeerIds].sort()) !== JSON.stringify([c.devices.android.id, c.devices.ios.id].sort())) fail('recovery not pending for exact receivers');
  } else if (recovery.outcome !== 'declined' || recovery.responderId !== c.devices.android.id
    || !['canonical', 'verified'].includes(recovery.resolutionSource)) fail('recovery decline/responder proof missing');
  const removals = operations.filter((op) => op.kind === 'removeDevice' && !baseline.has(op.requestId));
  if (phase === 'recover-pending') {
    if (removals.length) fail('removal preceded its pending boundary');
    return { recoveryId: recovery.requestId };
  }
  if (!removals.length && phase === 'both-pending' && !removalId) return null;
  if (removals.length !== 1) fail('removal Claim must be unique');
  const removal = removals[0];
  if (!removal.requestId || removal.requestId === recovery.requestId || removalId && removal.requestId !== removalId
    || removal.requesterId !== c.devices.web.id || removal.targetDeviceId !== c.devices.ios.id) fail('removal identity/target changed');
  if (phase === 'removed') {
    if (removal.outcome !== 'accepted' || removal.responderId !== c.devices.android.id
      || !['canonical', 'verified'].includes(removal.resolutionSource)) fail('removal accepted responder proof missing');
  } else if (!pending(removal) || JSON.stringify(removal.eligiblePeerIds) !== JSON.stringify([c.devices.android.id])) fail('removal must remain pending for B');
  return { recoveryId: recovery.requestId, removalId: removal.requestId };
}

/** Self-contained for page/JSHandle.evaluate; the runtime is read-only app ports plus test-owned cancellation. */
export async function waitForTest15FifoSnapshot(input, runtime) {
  // JSHandle.evaluate supplies its retained runtime as the first argument.
  if (input?.state && runtime?.phase) [input, runtime] = [runtime, input];
  if (!runtime) {
    const { AppState } = await import('/src/stores/app-state.ts');
    const { readOwnedMember } = await import('/src/features/liveboard/wasmOwnership.ts');
    const { readCoreProjection } = await import('/src/features/liveboard/coreProjection.ts');
    runtime = { state: AppState(), readOwnedMember,
      readCoreProjection: (member) => readCoreProjection({ member, identity: 'test15-read-only' }), setTimeout, clearTimeout };
  }
  const { state, readOwnedMember, readCoreProjection } = runtime;
  const manager = Object.hasOwn(runtime, 'originalManager') ? runtime.originalManager : state.appManager;
  const generation = Object.hasOwn(runtime, 'originalGeneration') ? runtime.originalGeneration : state.sessionGeneration;
  const setDeadline = runtime.setTimeout ?? globalThis.setTimeout, clearDeadline = runtime.clearTimeout ?? globalThis.clearTimeout;
  const signal = input.signal ?? runtime.controller?.signal;
  // This is the wait predicate only. The actual shared transition guard validates
  // the returned projection before any caller can open a decision gate.
  const reached = (p) => {
    if (!p.session?.ready || !p.session.member) return false;
    if (input.phase === 'snapshot') return true;
    const fresh = p.operations.filter((op) => !input.context.baselineRequestIds.includes(op.requestId));
    const r = fresh.filter((op) => op.kind === 'recover'), m = fresh.filter((op) => op.kind === 'removeDevice');
    if (input.phase === 'recover-pending') return r.length > 0;
    if (input.phase === 'both-pending') return r.length > 0 && m.length > 0;
    if (input.phase === 'recover-declined') return r.some((op) => op.outcome !== undefined);
    if (input.phase === 'removed') return m.some((op) => op.outcome !== undefined) || p.devices.length !== 3;
    throw new Error('Test15 unknown observation phase');
  };
  return new Promise((resolve, reject) => {
    let done = false, reading = false, again = false, unsubscribe = () => {};
    const finish = (error, projection) => {
      if (done) return;
      done = true; unsubscribe(); clearDeadline(deadline); signal?.removeEventListener('abort', abort);
      if (error) reject(error); else resolve(projection);
    };
    const abort = () => finish(signal.reason ?? new Error('Test15 observation cancelled'));
    const deadline = setDeadline(() => finish(new Error('Timed out waiting for Test15 canonical phase')), input.timeoutMs ?? 180_000);
    const check = async () => {
      if (done) return;
      if (reading) { again = true; return; }
      reading = true;
      try {
        if (state.appManager !== manager || state.sessionGeneration !== generation) throw new Error('Test15 session owner retired');
        if (state.syncError) throw new Error('Test15 current synchronization failed');
        if (state.isSynchronizing || state.requiredSyncPending || !state.currState) return;
        const current = state.currState;
        const projection = await readOwnedMember({ state: current }, (member) => readCoreProjection(member));
        if (done) return;
        if (state.appManager !== manager || state.sessionGeneration !== generation) throw new Error('Test15 session owner retired');
        if (state.syncError) throw new Error('Test15 current synchronization failed');
        if (state.currState !== current || state.isSynchronizing || state.requiredSyncPending) { again = true; return; }
        if (reached(projection)) finish(null, projection);
      } catch (error) { finish(error); }
      finally { reading = false; if (again && !done) { again = false; void check(); } }
    };
    if (signal?.aborted) return abort();
    signal?.addEventListener('abort', abort, { once: true });
    unsubscribe = state.$subscribe(() => { void check(); }, { detached: true, flush: 'sync' });
    void check();
  });
}

export async function runTest15FifoDecisions({ context, recoveryId }, ports) {
  const controller = new AbortController(); let primary, rejectActor, finished = false;
  const actorFailure = new Promise((_, reject) => { rejectActor = reject; });
  const failActor = (error) => { if (finished) return; primary ??= error; controller.abort(primary); rejectActor(primary); };
  const prefixes = { ready: 'ANDROID_TEST15_REMOVAL_APPROVER_READY', 'recovery-head': 'ANDROID_TEST15_RECOVERY_HEAD_VISIBLE_1_1',
    'recovery-declined': 'ANDROID_TEST15_RECOVERY_DECLINED_1_1', 'removal-head': 'ANDROID_TEST15_REMOVAL_REQUEST_VISIBLE',
    'removal-approved': 'ANDROID_TEST15_REMOVAL_APPROVED_RECOVERY_CLEARED' };
  const marker = async (stage, id, count) => {
    const line = await ports.waitMarker(stage, { signal: controller.signal });
    const prefix = `E2E: ${prefixes[stage]} `;
    if (typeof line !== 'string' || !line.startsWith(prefix)) throw new Error('Test15 actual head marker prefix mismatch');
    let metadata; try { metadata = JSON.parse(line.slice(prefix.length)); } catch { throw new Error('Test15 incomplete marker metadata'); }
    const expected = { ...context.scope, ...(id ? { claimId: id } : {}), count };
    if (Object.keys(metadata).length !== Object.keys(expected).length || Object.entries(expected).some(([key, value]) => metadata[key] !== value)) throw new Error('Test15 marker scope/UUID/count mismatch');
  };
  const observe = async (phase, removalId) => {
    const group = { context, phase, recoveryId, removalId };
    const projection = await ports.observe(group, controller.signal);
    const result = assertTest15FifoTransition(group, projection);
    if (!result) throw new Error('Test15 canonical observation did not reach its phase');
    return result;
  };
  const work = (async () => {
    await ports.passiveResult;
    const actor = await ports.startActor(context);
    void actor.result.then(() => failActor(new Error('Test15 actor ended before the CLI tail')), failActor);
    await marker('ready', null, 0);
    await ports.requestRemoval(context);
    const { removalId } = await observe('both-pending');
    await marker('recovery-head', recoveryId, 2);
    await ports.allowGate({ name: 'test15-recover-decline', cycle: 1, requestId: recoveryId, scope: context.scope });
    await marker('recovery-declined', recoveryId, 1);
    await observe('recover-declined', removalId);
    await marker('removal-head', removalId, 1);
    await ports.allowGate({ name: 'test15-remove-approve', cycle: 1, requestId: removalId, scope: context.scope });
    await marker('removal-approved', removalId, 0);
    return await observe('removed', removalId);
  })();
  try { return await Promise.race([work, actorFailure]); }
  catch (error) { primary ??= error; throw primary; }
  finally {
    finished = true;
    controller.abort(primary ?? new Error('Test15 fragment complete'));
    try { await ports.drain(); } catch (error) { if (!primary) throw error; }
  }
}
