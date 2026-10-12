/** Serializable page callback: subscribe to existing state, never invoke Core/sync. */
export function waitForCanonicalTerminal({ passId, requestId, expectedStatus, timeoutMs = 120000, allowExistingSyncError = false, store = globalThis.__TEST50_STORE }) {
  return new Promise((resolve, reject) => {
    let unsubscribe = () => {}; let finished = false;
    const initialSyncError = allowExistingSyncError ? store.syncError : null;
    const initialState = store.currState;
    let freshReconnect = !initialSyncError;
    let initialErrorPending = Boolean(initialSyncError);
    const finish = (error, result) => { if (finished) return; finished = true; clearTimeout(timer); unsubscribe(); error ? reject(error) : resolve(result); };
    const timer = setTimeout(() => finish(new Error('Web canonical terminal state did not settle for the exact deletion identity')), timeoutMs);
    const inspect = () => {
      try {
        if (!store.syncError) initialErrorPending = false;
        if (store.isSynchronizing) { freshReconnect = true; return; }
        if (store.currState !== initialState) freshReconnect = true;
        if (store.syncError && initialErrorPending && store.syncError === initialSyncError) return;
        if (store.syncError) return finish(new Error('Web synchronization failed before its deletion terminal postcondition'));
        if (!freshReconnect) return;
        const states = JSON.parse(store.currState.as_vault().as_member().deletion_states_json());
        const terminal = states.find((state) => state.secretId === passId && (!requestId || state.requestId === requestId) && ['Done', 'Declined'].includes(state.clientStatus));
        if (!terminal) return;
        if (terminal.clientStatus !== expectedStatus) return finish(new Error('Web canonical deletion outcome contradicts the declared first response'));
        finish(null, { secretId: terminal.secretId, requestId: terminal.requestId, clientStatus: terminal.clientStatus });
      } catch { finish(new Error('Web canonical deletion metadata could not be read (payload omitted)')); }
    };
    unsubscribe = store.$subscribe(inspect, { flush: 'sync', detached: true });
    inspect();
  });
}

export function bindWebFailureGuard(page, fail) {
  const pageError = () => fail(new Error('Web emitted an uncaught browser error during deletion'));
  const crash = () => fail(new Error('Web renderer crashed during deletion'));
  const consoleError = (message) => { if (message.type() === 'error' && /null pointer passed to rust/i.test(message.text())) pageError(); };
  page.on('pageerror', pageError); page.on('crash', crash); page.on('console', consoleError);
  return () => { page.off('pageerror', pageError); page.off('crash', crash); page.off('console', consoleError); };
}

/** Serializable ownership postcondition for the exact object rendered by Secrets. */
export function browserOwnedSecretUsable({ name, id, owned = globalThis.__TEST50_OWNED_DELETE_SECRET }) {
  return Boolean(owned?.__wbg_ptr) && owned.name === name && owned.id_str() === id;
}
