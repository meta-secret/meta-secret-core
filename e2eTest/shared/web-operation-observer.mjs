/** Serializable test-owned observer: only forwards the UI's actual operation. Never invokes Core eagerly. */
export async function installWebOperationObserver(options) {
  const registry = options.registry ?? (globalThis.__METASECRET_E2E_OPERATIONS ??= {});
  const manager = options.manager ?? (await import('/src/stores/app-state.ts')).AppState().appManager;
  const { scope, method, expectedId, expectedName, borrowed = false, holdResult = false } = options;
  if (!scope || !expectedId || registry[scope] || typeof manager?.[method] !== 'function') throw new Error('Invalid/duplicate actual operation observer');
  const original = manager[method];
  const entry = { phase: 'armed', calls: 0, borrowedAlive: null, expectedId, expectedName };
  registry[scope] = entry;
  const identity = (value) => value?.id_str?.() ?? value?.wasm_id_str?.();
  const wrapper = async function (...args) {
    let id;
    try { id = identity(args[0]); } catch { /* A foreign/unreadable call cannot satisfy this observer. */ }
    if (id !== expectedId || entry.phase !== 'armed') return original.apply(this, args);
    entry.calls++; entry.phase = 'entered';
    const selected = args[0], pointer = selected?.__wbg_ptr;
    let name; try { name = selected?.name; } catch { /* Captured failure is an oracle, not an operation mutation. */ }
    try {
      const value = await original.apply(this, args);
      if (borrowed) {
        try { entry.borrowedAlive = Boolean(pointer && selected.__wbg_ptr === pointer && identity(selected) === expectedId && selected.name === name && (expectedName == null || name === expectedName)); }
        catch { entry.borrowedAlive = false; }
      }
      entry.phase = holdResult ? 'held' : 'fulfilled';
      if (manager[method] === wrapper) manager[method] = original;
      if (holdResult) await new Promise((resolve) => { entry.release = resolve; });
      return value;
    } catch (error) {
      entry.phase = 'rejected';
      if (manager[method] === wrapper) manager[method] = original;
      throw error;
    }
  };
  entry.restore = () => { if (manager[method] === wrapper) manager[method] = original; entry.release?.(); };
  manager[method] = wrapper;
}

export function readWebOperationObserver({ scope, registry = globalThis.__METASECRET_E2E_OPERATIONS }) {
  const entry = registry?.[scope];
  if (!entry) throw new Error('Missing scoped actual operation observer');
  return { phase: entry.phase, calls: entry.calls, expectedId: entry.expectedId, expectedName: entry.expectedName, borrowedAlive: entry.borrowedAlive };
}

export function releaseWebOperationObserver({ scope, registry = globalThis.__METASECRET_E2E_OPERATIONS }) {
  const entry = registry?.[scope];
  if (!entry || entry.phase !== 'held' || !entry.release) throw new Error('Actual return is not held');
  entry.release(); delete entry.release; entry.phase = 'fulfilled';
}

export function disposeWebOperationObserver({ scope, registry = globalThis.__METASECRET_E2E_OPERATIONS }) {
  registry?.[scope]?.restore?.(); if (registry) delete registry[scope];
}

export async function waitWebOperation(page, scope, { held = false, timeout = 120_000 } = {}) {
  await page.waitForFunction(({ scope, held }) => {
    const entry = globalThis.__METASECRET_E2E_OPERATIONS?.[scope];
    return entry && (entry.phase === 'rejected' || entry.phase === (held ? 'held' : 'fulfilled'));
  }, { scope, held }, { timeout });
  const result = await page.evaluate(readWebOperationObserver, { scope });
  if (result.phase === 'rejected') throw new Error('Actual UI operation rejected (payload omitted)');
  if (result.calls !== 1) throw new Error('Actual UI operation entry count mismatch');
  return result;
}

/** Forward ORIGINAL WebAuthn get with a real AbortSignal. Browser API abort, never a simulated OS-button claim. */
export function installWebAuthnAbort({ credentials = globalThis.navigator.credentials, registry = globalThis, scope }) {
  if (!scope || registry[scope]) throw new Error('Duplicate browser authentication observer');
  const original = credentials.get;
  const entry = { phase: 'armed', outcome: null, controller: null };
  registry[scope] = entry;
  const wrapper = function (options) {
    if (entry.phase !== 'armed') return original.call(this, options);
    if (options.signal) throw new Error('Refusing to replace an existing WebAuthn AbortSignal');
    entry.controller = new AbortController(); entry.phase = 'entered';
    return original.call(this, { ...options, signal: entry.controller.signal }).then((value) => {
      entry.phase = 'settled'; entry.outcome = 'credential'; return value;
    }, (error) => {
      entry.phase = 'settled'; entry.outcome = error?.name; throw error;
    }).finally(() => { if (credentials.get === wrapper) credentials.get = original; });
  };
  entry.restore = () => { if (credentials.get === wrapper) credentials.get = original; };
  credentials.get = wrapper;
}

export function abortWebAuthn({ registry = globalThis, scope }) {
  const entry = registry[scope];
  if (entry?.phase !== 'entered' || !entry.controller) throw new Error('No actual entered WebAuthn request to abort');
  entry.controller.abort();
}

export function disposeWebAuthnAbort({ registry = globalThis, scope }) {
  registry[scope]?.restore?.(); delete registry[scope];
}
