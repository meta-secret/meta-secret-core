/** Browser-serializable, event-driven read-only projection observation. No RPC/refresh or command. */
export async function waitForFifoCanonicalSnapshot(input, runtime) {
  if (!runtime) {
    const { AppState } = await import('/src/stores/app-state.ts');
    const { readOwnedMember } = await import('/src/features/liveboard/wasmOwnership.ts');
    const { readCoreProjection } = await import('/src/features/liveboard/coreProjection.ts');
    runtime = { state: AppState(), readOwnedMember, readCoreProjection, setTimeout, clearTimeout };
  }
  const { state, readOwnedMember, readCoreProjection } = runtime;
  const originalManager = state.appManager, originalGeneration = state.sessionGeneration;
  const setDeadline = runtime.setTimeout ?? globalThis.setTimeout, clearDeadline = runtime.clearTimeout ?? globalThis.clearTimeout;
  const pending = (op) => ['pending', 'needApprove'].includes(op.clientStatus) && op.outcome === undefined;
  const ready = (projection) => {
    if (!projection.session?.ready || !projection.session.member) return false;
    if (input.mode === 'snapshot') return true;
    if (input.mode === 'pending') {
      const baseline = new Set(input.baselineRequestIds ?? []), expected = input.expected ?? [];
      const operations = projection.operations.filter((op) => op.kind === 'recover');
      if (operations.some((op) => baseline.has(op.requestId) && pending(op))) throw new Error('Old pending recovery crosses the current cycle boundary');
      const fresh = operations.filter((op) => !baseline.has(op.requestId));
      for (const op of fresh) {
        if (!expected.some((item) => item.requesterId === op.requesterId && item.passId === op.secretId)) throw new Error('Foreign current-cycle recovery submission');
        if (!pending(op)) throw new Error('Current-cycle recovery submission is terminal, not pending');
      }
      for (const item of expected) {
        const matches = fresh.filter((op) => op.requesterId === item.requesterId && op.secretId === item.passId);
        if (matches.length > 1) throw new Error('Current-cycle recovery submission is not unique');
        if (!matches.length) return false;
      }
      return fresh.length === expected.length && expected.length > 0;
    }
    if (input.mode === 'terminal') {
      const matches = projection.operations.filter((op) => op.kind === 'recover' && op.requestId === input.requestId);
      if (matches.length > 1) throw new Error('Terminal recovery UUID is not unique');
      if (!matches.length) return false;
      const actual = matches[0];
      if (actual.requesterId !== input.expectedRequest.requesterId || actual.secretId !== input.expectedRequest.passId) throw new Error('Terminal recovery UUID identity changed');
      if (actual.outcome === undefined) return false;
      if (actual.outcome !== input.outcome) throw new Error('Contradictory canonical recovery outcome');
      if (!['canonical', 'verified'].includes(actual.resolutionSource)) return false;
      for (const expected of input.pendingRequests ?? []) {
        const found = projection.operations.filter((op) => op.kind === 'recover' && op.requestId === expected.requestId);
        if (found.length !== 1 || found[0].requesterId !== expected.requesterId || found[0].secretId !== expected.passId || !pending(found[0])) throw new Error('Other submitted recovery is no longer pending');
      }
      return true;
    }
    throw new Error('Unknown FIFO observation mode');
  };
  return new Promise((resolve, reject) => {
    let done = false, reading = false, again = false, unsubscribe = () => {};
    const deadline = setDeadline(() => finish(new Error('Timed out waiting for current canonical FIFO projection')), input.timeoutMs ?? 120_000);
    const finish = (error, projection) => {
      if (done) return;
      done = true; unsubscribe(); clearDeadline(deadline);
      if (error) reject(error); else resolve(projection);
    };
    const check = async () => {
      if (done) return;
      if (reading) { again = true; return; }
      reading = true;
      try {
        if (state.appManager !== originalManager || state.sessionGeneration !== originalGeneration) throw new Error('FIFO native session retired during canonical observation');
        if (state.syncError) throw new Error('FIFO current synchronization failed (payload omitted)');
        if (state.isSynchronizing || state.requiredSyncPending || !state.currState) return;
        const current = state.currState;
        const projection = await readOwnedMember({ state: current }, (member) => readCoreProjection({ member, identity: 'e2e-fifo-read-only' }));
        if (state.appManager !== originalManager || state.sessionGeneration !== originalGeneration) throw new Error('FIFO native session retired during canonical observation');
        if (state.currState !== current || state.isSynchronizing || state.requiredSyncPending) { again = true; return; }
        if (state.syncError) throw new Error('FIFO current synchronization failed (payload omitted)');
        if (ready(projection)) finish(null, projection);
      } catch (error) { finish(error); }
      finally { reading = false; if (again && !done) { again = false; void check(); } }
    };
    unsubscribe = state.$subscribe(() => { void check(); }, { detached: true, flush: 'sync' });
    void check();
  });
}
