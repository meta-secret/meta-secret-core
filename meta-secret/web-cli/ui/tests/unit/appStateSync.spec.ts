import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { MasterKeyManager } from 'meta-secret-web-cli';
import type { TransportSk, WasmApplicationManager, WasmApplicationState } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { deferred, managerFixture, stateFixture } from './liveboardCoreFixtures';

const initialization = vi.hoisted(() => ({ manager: vi.fn(), authenticate: vi.fn() }));

vi.mock('meta-secret-web-cli', () => ({
  default: vi.fn(),
  ApplicationStateInfo: { Local: 0, Member: 1 },
  MasterKeyManager: { from_pure_sk: vi.fn(() => ({})) },
  WasmApplicationManager: { init_wasm_with_device_and_environment: initialization.manager },
}));
vi.mock('@/stores/auth', () => ({
  useAuthStore: () => ({ masterKey: 'unit-test-key', authenticateWithPasskey: initialization.authenticate }),
}));

beforeEach(() => {
  setActivePinia(createPinia());
});

describe('Liveboard shared AppState generation lane', () => {
  it('shares one manager initialization between concurrent callers', async () => {
    const store = AppState();
    const manager = managerFixture();
    const creating = deferred<WasmApplicationManager>();
    initialization.manager.mockReset().mockReturnValue(creating.promise);
    const first = store.appStateInit();
    const second = store.appStateInit();
    await Promise.resolve();
    await Promise.resolve();
    const started = initialization.manager.mock.calls.length;
    creating.resolve(manager);
    await Promise.all([first, second]);
    expect(started).toBe(1);
    expect(manager.get_state).toHaveBeenCalledOnce();
  });

  it('serializes overlapping get_state calls through the same store lane', async () => {
    const store = AppState();
    const reading = deferred<WasmApplicationState>();
    let active = 0;
    let maximum = 0;
    let calls = 0;
    const manager = managerFixture();
    vi.mocked(manager.get_state).mockImplementation(() => {
      maximum = Math.max(maximum, ++active);
      const result = ++calls === 1 ? reading.promise : Promise.resolve(stateFixture().state);
      return result.finally(() => {
        active--;
      });
    });
    store.appManager = manager;
    const first = store.updateState();
    const second = store.updateState();
    await Promise.resolve();
    reading.resolve(stateFixture().state);
    await Promise.all([first, second]);
    expect(maximum).toBe(1);
  });

  it('rejects an old get_state publication after registration atomically supplies a newer root', async () => {
    const store = AppState();
    const reading = deferred<WasmApplicationState>();
    const manager = managerFixture();
    vi.mocked(manager.get_state).mockReturnValue(reading.promise);
    store.appManager = manager;
    const pending = store.updateState();
    await Promise.resolve();
    const registrationRoot = stateFixture().state;
    store.updateStateWith(registrationRoot);
    reading.resolve(stateFixture().state);
    await pending;
    expect(store.currState).toEqual(registrationRoot);
  });
});
describe('Vault synchronization visibility', () => {
  it('keeps loading active until Core returns the reconciled state', async () => {
    const store = AppState();
    let finish: (state: WasmApplicationState) => void = () => {};
    store.appManager = {
      get_state: () =>
        new Promise<WasmApplicationState>((resolve) => {
          finish = resolve;
        }),
    } as unknown as WasmApplicationManager;
    const next = { as_info: () => 1 } as unknown as WasmApplicationState;
    const refresh = store.updateState();
    expect(store.isSynchronizing).toBe(true);
    expect(store.currState).toBeNull();
    finish(next);
    await refresh;
    expect(store.isSynchronizing).toBe(false);
    expect(store.currState).toEqual(next);
    expect(store.syncError).toBe('');
  });

  it('blocks cached state after a failed refresh and clears the error after retry', async () => {
    const store = AppState();
    const cached = { as_info: () => 1 } as unknown as WasmApplicationState;
    store.currState = cached;
    const next = { as_info: () => 1 } as unknown as WasmApplicationState;
    const getState = vi.fn().mockRejectedValueOnce(new Error('private-error-marker')).mockResolvedValue(next);
    store.appManager = { get_state: getState } as unknown as WasmApplicationManager;
    await expect(store.updateState()).rejects.toThrow();
    expect(store.isSynchronizing).toBe(false);
    expect(store.currState).toEqual(cached);
    expect(store.syncError).toContain('Retry');
    expect(store.syncError).not.toContain('private-error-marker');
    await store.updateState();
    expect(store.syncError).toBe('');
    expect(store.currState).toEqual(next);
  });
});

describe('AppState consuming initializer migration retry', () => {
  it('retries the legacy deviceType migration with a fresh actually passed TransportSk', async () => {
    const store = AppState();
    const manager = managerFixture();
    const root = stateFixture().state;
    vi.mocked(manager.get_state).mockResolvedValue(root);
    const consumed = new WeakSet<TransportSk>();
    const passed: TransportSk[] = [];
    const capturedKeys: string[] = [];
    const factory = vi.spyOn(MasterKeyManager, 'from_pure_sk').mockImplementation((key) => {
      capturedKeys.push(key);
      return {
        free: vi.fn(),
        [Symbol.dispose]: vi.fn(),
        0: { free: vi.fn(), [Symbol.dispose]: vi.fn() },
      };
    });
    const cleanup = vi.spyOn(store, 'clearAllMetaSecretIndexedDb').mockResolvedValue(undefined);
    const previousInitialize = initialization.manager.getMockImplementation();
    initialization.authenticate.mockClear();
    initialization.manager.mockReset().mockImplementation(async (transport: TransportSk) => {
      passed.push(transport);
      if (consumed.has(transport)) throw new Error('ConsumedTransportWrapper');
      consumed.add(transport);
      if (passed.length === 1) throw new Error('missing field `deviceType`');
      return manager;
    });
    try {
      const initialized = await store.appStateInit().then(
        () => true,
        () => false,
      );
      expect(initialization.manager).toHaveBeenCalledTimes(2);
      expect(cleanup).toHaveBeenCalledOnce();
      expect(initialization.authenticate).not.toHaveBeenCalled();
      expect(passed[0] !== passed[1]).toBe(true);
      expect(initialized).toBe(true);
      expect(capturedKeys.length === 2 && capturedKeys.every((key) => key === capturedKeys[0])).toBe(true);
      expect(factory).toHaveBeenCalledTimes(2);
      expect(passed.every((transport) => consumed.has(transport))).toBe(true);
      for (const transport of passed) expect(transport.free).not.toHaveBeenCalled();
      expect(store.appManager).toBe(manager);
      expect(store.currState).toBe(root);
      expect(manager.get_state).toHaveBeenCalledOnce();
      expect(store.syncError).toBe('');
    } finally {
      factory.mockRestore();
      cleanup.mockRestore();
      initialization.manager.mockReset();
      if (previousInitialize) initialization.manager.mockImplementation(previousInitialize);
    }
  });
});
