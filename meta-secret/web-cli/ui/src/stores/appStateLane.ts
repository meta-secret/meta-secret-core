import { markRaw } from 'vue';
import type { WasmApplicationManager, WasmApplicationState } from 'meta-secret-web-cli';

export interface AppStateLaneStore {
  appManager: WasmApplicationManager | null;
  currState: WasmApplicationState | null;
  sessionGeneration: number;
  requiredSyncPending: boolean;
  syncError: string;
  beginSync(): void;
  endSync(): void;
}
type Identity = { key: string | null | undefined; authenticated: boolean | undefined };
type Context = { manager: WasmApplicationManager | null; identity: Identity; generation: number; publication: number };
type Refresh = { requiredSync?: boolean; coalesce?: boolean };
const failure = () => new Error('OperationUnavailable');
const message = 'Could not synchronize Vault. Retry to apply pending changes.';
const sameIdentity = (left: Identity, right: Identity) =>
  left.key === right.key && left.authenticated === right.authenticated;

/** A private physical lane; root handles are detached on invalidation, never freed here. */
export function createAppStateLane({ store, identity }: { store: AppStateLaneStore; identity(): Identity }) {
  let tail: Promise<void> | null = null;
  let publication = 0,
    requiredRevision = 0;
  let managerIdentity: { manager: WasmApplicationManager; identity: Identity } | null = null;
  let initialization: { identity: Identity; generation: number; promise: Promise<void> } | null = null;
  type Flight = {
    context: Context;
    required: boolean;
    revision: number;
    phase: 'queued' | 'syncing' | 'reading';
    promise: Promise<WasmApplicationState | null>;
  };
  let flight: Flight | null = null;
  function serial<T>(action: () => Promise<T>): Promise<T> {
    const previous = tail;
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    tail = gate;
    let result: Promise<T>;
    try {
      result = previous ? previous.then(action) : action();
    } catch (error) {
      result = Promise.reject(error);
    }
    void result.then(release, release);
    void gate.then(() => {
      if (tail === gate) tail = null;
    });
    return result;
  }
  const capture = (): Context => ({
    manager: store.appManager,
    identity: identity(),
    generation: store.sessionGeneration,
    publication,
  });
  const current = (context: Context) =>
    context.manager === store.appManager &&
    context.generation === store.sessionGeneration &&
    context.publication === publication &&
    sameIdentity(context.identity, identity()) &&
    (managerIdentity?.manager !== context.manager || sameIdentity(managerIdentity.identity, context.identity));
  function invalidate() {
    store.sessionGeneration++;
    publication++;
    requiredRevision++;
    initialization = null;
    flight = null;
    managerIdentity = null;
    store.appManager = null;
    store.currState = null;
    store.requiredSyncPending = false;
    store.syncError = '';
  }
  function publish(state: WasmApplicationState) {
    publication++;
    store.currState = markRaw(state);
    return store.currState;
  }
  function refresh({ requiredSync = false, coalesce = true }: Refresh = {}): Promise<WasmApplicationState | null> {
    const context = capture();
    if (managerIdentity?.manager === context.manager && !sameIdentity(managerIdentity.identity, context.identity)) {
      invalidate();
      return Promise.resolve(null);
    }
    if (requiredSync) store.requiredSyncPending = true;
    if (!context.manager) return Promise.resolve(null);
    if (coalesce && flight && current(flight.context)) {
      if (!requiredSync || (flight.required && flight.phase !== 'reading')) return flight.promise;
      if (flight.phase === 'queued') {
        flight.required = true;
        flight.revision = ++requiredRevision;
        return flight.promise;
      }
    }
    const revision = requiredSync ? ++requiredRevision : requiredRevision;
    let resolve!: (state: WasmApplicationState | null) => void, reject!: (error: unknown) => void;
    const promise = new Promise<WasmApplicationState | null>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    const owner: Flight = { context, required: requiredSync, revision, phase: 'queued', promise };
    flight = owner;
    store.beginSync();
    void serial(async () => {
      try {
        if (!current(context)) return null;
        if (owner.required) {
          owner.phase = 'syncing';
          if (typeof context.manager!.sync_now !== 'function') throw failure();
          await context.manager!.sync_now();
          if (!current(context)) return null;
        }
        owner.phase = 'reading';
        const next = await context.manager!.get_state();
        if (!current(context)) return null;
        store.currState = markRaw(next);
        if (owner.required && owner.revision === requiredRevision) store.requiredSyncPending = false;
        if (!store.requiredSyncPending) store.syncError = '';
        return store.currState;
      } catch {
        if (current(context)) store.syncError = message;
        throw failure();
      } finally {
        store.endSync();
        if (flight === owner) flight = null;
      }
    }).then(resolve, reject);
    return promise;
  }
  function initialize(
    create: (key: string, isCurrent: () => boolean) => Promise<WasmApplicationManager>,
  ): Promise<void> {
    const selected = identity();
    if (
      initialization &&
      initialization.generation === store.sessionGeneration &&
      sameIdentity(initialization.identity, selected)
    )
      return initialization.promise;
    invalidate();
    const context = capture();
    store.beginSync();
    let resolve!: () => void, reject!: (error: unknown) => void;
    const promise = new Promise<void>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    const owner = { identity: selected, generation: context.generation, promise };
    initialization = owner;
    void serial(async () => {
      try {
        if (!current(context) || !selected.key) throw failure();
        const manager = await create(selected.key, () => current(context));
        if (!current(context)) return;
        const state = await manager.get_state();
        if (!current(context)) return;
        store.appManager = markRaw(manager);
        store.currState = markRaw(state);
        managerIdentity = { manager: store.appManager, identity: selected };
        if (!store.requiredSyncPending) store.syncError = '';
      } catch {
        if (current(context)) store.syncError = message;
        throw failure();
      } finally {
        store.endSync();
        if (initialization === owner) initialization = null;
      }
    }).then(resolve, reject);
    return promise;
  }
  function clean(action: (manager: WasmApplicationManager | null) => Promise<void>) {
    const manager = store.appManager;
    if (managerIdentity?.manager === manager && !sameIdentity(managerIdentity.identity, identity())) {
      invalidate();
      return Promise.reject(failure());
    }
    invalidate();
    const context = capture();
    store.beginSync();
    return serial(async () => {
      try {
        if (!current(context)) throw failure();
        await action(manager);
        if (!current(context)) throw failure();
      } finally {
        store.endSync();
      }
    });
  }
  async function authorize(action: (manager: WasmApplicationManager) => Promise<string>) {
    const context = capture();
    if (!context.manager || !current(context)) throw failure();
    const token = await action(context.manager);
    if (!current(context)) throw failure();
    return token;
  }
  return { refresh, initialize, invalidate, publish, clean, authorize };
}
