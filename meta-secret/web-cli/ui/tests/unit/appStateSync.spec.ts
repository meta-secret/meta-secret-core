import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import type { WasmApplicationManager, WasmApplicationState } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';

vi.mock('meta-secret-web-cli', () => ({
  default: vi.fn(),
  ApplicationStateInfo: { Local: 0, Member: 1 },
  MasterKeyManager: {},
}));
vi.mock('@/stores/auth', () => ({ useAuthStore: () => ({}) }));

beforeEach(() => {
  setActivePinia(createPinia());
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
