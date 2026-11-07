import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises } from '@vue/test-utils';
import { effectScope, reactive } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import type { WasmApplicationManager } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { useVaultSession } from '@/features/liveboard/useVaultSession';
import type { StateInvalidationControllerOptions } from '@/utils/stateInvalidation';
import { deferred, managerFixture, platformFixture, stateFixture } from './liveboardCoreFixtures';

const mocks = vi.hoisted(() => ({
  init: vi.fn(),
  manager: null as WasmApplicationManager | null,
  auth: { isAuthenticated: true, masterKey: 'key-a' as string | null },
  initialize: vi.fn(),
}));
vi.mock('@/stores/auth', () => ({ useAuthStore: () => mocks.auth }));
vi.mock('meta-secret-web-cli', () => ({
  default: mocks.init,
  ApplicationStateInfo: { Local: 0, Member: 1, Outsider: 2, VaultNotExists: 3 },
  MasterKeyManager: { from_pure_sk: vi.fn(() => ({})) },
  WasmApplicationManager: { init_wasm_with_device_and_environment: mocks.initialize },
}));
const disposals: Array<() => void> = [];
beforeEach(() => {
  vi.clearAllMocks();
  setActivePinia(createPinia());
  mocks.auth = reactive({ isAuthenticated: true, masterKey: 'key-a' });
  mocks.manager = managerFixture();
  mocks.init.mockResolvedValue(undefined);
  mocks.initialize.mockImplementation(async () => mocks.manager);
});
afterEach(() => {
  disposals.splice(0).forEach((dispose) => dispose());
});

async function setup() {
  const store = AppState();
  const connection = { connect: vi.fn(), disconnect: vi.fn(), refreshNow: vi.fn(async () => {}) };
  const createInvalidation = vi.fn<(options: StateInvalidationControllerOptions) => typeof connection>(
    () => connection,
  );
  const scope = effectScope();
  const session = scope.run(() =>
    useVaultSession({ store, auth: mocks.auth, createInvalidation, platform: platformFixture() }),
  )!;
  disposals.push(() => {
    session.dispose();
    scope.stop();
  });
  await flushPromises();
  return { store, session, connection, createInvalidation, manager: mocks.manager! };
}

describe('Liveboard parent session over the actual AppState owner', () => {
  it('keeps one await-init, manager, controller and authenticated stream across child routes', async () => {
    const s = await setup();
    const owner = s.session.controller;
    for (const route of ['secrets', 'profile', 'home', 'secrets', 'home'] as const)
      s.session.controller.navigate(route);
    await flushPromises();
    expect(mocks.init).toHaveBeenCalledOnce();
    expect(mocks.initialize).toHaveBeenCalledOnce();
    expect(s.createInvalidation).toHaveBeenCalledOnce();
    expect(s.connection.connect).toHaveBeenCalledExactlyOnceWith('семья@example.test');
    expect(s.session.controller).toBe(owner);
  });

  it('serializes required sync before read and coalesces a burst without losing required sync', async () => {
    const s = await setup();
    const syncing = deferred<void>();
    const order: string[] = [];
    vi.mocked(s.manager.sync_now).mockImplementation(() => {
      order.push('sync');
      return syncing.promise;
    });
    vi.mocked(s.manager.get_state).mockImplementation(async () => {
      order.push('read');
      return stateFixture().state;
    });
    const first = s.session.refresh({ requiredSync: true });
    const second = s.session.refresh({ requiredSync: true });
    expect(s.session.state.value.ready).toBe(false);
    await Promise.resolve();
    expect(order).toEqual(['sync']);
    syncing.resolve();
    await Promise.all([first, second]);
    expect(order[0]).toBe('sync');
    expect(order.filter((value) => value === 'read')).toHaveLength(1);
    expect(s.session.state.value.ready).toBe(true);
  });

  it('keeps required-sync failure blocked after cached read and manual error dismissal, until real sync succeeds', async () => {
    const s = await setup();
    vi.mocked(s.manager.sync_now).mockRejectedValueOnce(new Error('private-sync-error'));
    await s.session.refresh({ requiredSync: true }).catch(() => {});
    vi.mocked(s.manager.get_state).mockClear();
    expect(await s.store.updateState()).not.toBeNull();
    expect(s.manager.get_state).toHaveBeenCalledOnce();
    s.session.dismissError();
    expect(s.session.state.value.ready).toBe(false);
    expect(s.session.controller.secrets.state.value.blocked).toBe(true);
    expect(JSON.stringify(s.session.state.value)).not.toContain('private-sync-error');
    await s.session.refresh({ requiredSync: true });
    expect(s.session.state.value.ready).toBe(true);
  });

  it('invalidates synchronously when masterKey changes while the Boolean auth state stays true', async () => {
    const s = await setup();
    s.session.controller.navigate('secrets');
    await s.session.controller.secrets.primary('pass-a');
    const held = s.session.controller.secrets.state.value.cards['pass-a'];
    expect(held?.value).not.toBeNull();
    mocks.auth.masterKey = 'key-b';
    expect(mocks.auth.isAuthenticated).toBe(true);
    expect(held?.value).toBeNull();
    expect(s.session.state.value.ready).toBe(false);
    expect(s.connection.disconnect).toHaveBeenCalled();
    expect(s.manager.delete_secret_checked).not.toHaveBeenCalled();
  });

  it('ignores old read and queued online/SSE callbacks after disposal and a new manager epoch', async () => {
    const s = await setup();
    const oldRead = deferred<ReturnType<typeof stateFixture>['state']>();
    vi.mocked(s.manager.get_state).mockReturnValue(oldRead.promise);
    const refreshing = s.session.refresh({ requiredSync: false });
    await Promise.resolve();
    s.session.dispose();
    const replacement = stateFixture().state;
    s.store.updateStateWith(replacement);
    const oldReadCount = vi.mocked(s.manager.get_state).mock.calls.length;
    await s.createInvalidation.mock.calls[0]![0].refresh();
    window.dispatchEvent(new Event('online'));
    oldRead.resolve(stateFixture().state);
    await refreshing.catch(() => {});
    await flushPromises();
    expect(s.store.currState).toEqual(replacement);
    expect(s.manager.get_state).toHaveBeenCalledTimes(oldReadCount);
    expect(s.session.state.value.ready).toBe(false);
    expect(s.connection.disconnect).toHaveBeenCalled();
  });

  it('uses the existing cleanDatabase then reinitialization flow once for confirmed local reset', async () => {
    const s = await setup();
    const local = stateFixture().state;
    local.as_info = () => 0;
    local.is_local = () => true;
    local.is_vault = () => false;
    vi.mocked(s.manager.clean_up_database).mockImplementation(async () => {
      vi.mocked(s.manager.get_state).mockResolvedValue(local);
    });
    s.session.controller.requestReset();
    await s.session.controller.confirmReset();
    expect(s.manager.clean_up_database).toHaveBeenCalledOnce();
    expect(mocks.initialize).toHaveBeenCalledTimes(2);
    expect(s.store.isLocal).toBe(true);
    expect(s.session.state.value.ready).toBe(false);
    expect(s.manager.generate_user_creds).not.toHaveBeenCalled();
    expect(s.manager.sign_up).not.toHaveBeenCalled();
  });

  it('releases a retired reset after key replacement and keeps the new manager/controller usable', async () => {
    const store = AppState();
    const platform = platformFixture();
    const scope = effectScope();
    const session = scope.run(() =>
      useVaultSession({
        store,
        auth: mocks.auth,
        platform,
        createInvalidation: () => ({ connect: vi.fn(), disconnect: vi.fn(), refreshNow: vi.fn(async () => {}) }),
      }),
    )!;
    disposals.push(() => {
      session.dispose();
      scope.stop();
    });
    await flushPromises();
    const oldManager = mocks.manager!;
    const oldController = session.controller;
    const oldIdentity = session.state.value.identity;
    const oldRoot = store.currState;
    expect(session.state.value.ready).toBe(true);
    expect(oldIdentity).not.toBe('');
    oldController.navigate('secrets');
    await oldController.secrets.primary('pass-a');
    await oldController.secrets.copy('pass-a');
    const held = oldController.secrets.state.value.cards['pass-a'];
    expect(held).toMatchObject({ value: { kind: 'text', text: 'private marker' }, copied: true });
    oldController.create.open();
    oldController.create.change({ name: 'private name', value: 'private draft' });
    const cleanup = deferred<void>();
    vi.mocked(oldManager.clean_up_database).mockReturnValue(cleanup.promise);
    oldController.requestReset();
    const resetting = oldController.confirmReset();
    expect(session.state.value.resetting).toBe(true);
    expect(held).toMatchObject({ value: null, copied: false });
    expect(oldController.create.state.value.draft.value).toBe('');
    expect(oldManager.clean_up_database).toHaveBeenCalledOnce();

    const nextManager = managerFixture();
    const nextRoot = stateFixture().state;
    vi.mocked(nextManager.get_state).mockResolvedValue(nextRoot);
    vi.mocked(nextManager.show_local_secret).mockResolvedValue('new owner value');
    mocks.manager = nextManager;
    mocks.auth.masterKey = 'key-b';
    expect(mocks.auth.isAuthenticated).toBe(true);
    expect(session.state.value.ready).toBe(false);
    expect(store.currState).not.toBe(oldRoot);
    cleanup.resolve();
    await resetting;
    await flushPromises();
    expect(store.appManager).toBe(nextManager);
    expect(store.currState).toBe(nextRoot);
    expect(mocks.initialize).toHaveBeenCalledTimes(2);
    expect(oldManager.clean_up_database).toHaveBeenCalledOnce();
    expect(nextManager.clean_up_database).not.toHaveBeenCalled();
    expect(platform.authenticate).not.toHaveBeenCalled();
    expect(session.state.value.resetting).toBe(false);
    expect(session.state.value.ready).toBe(true);
    expect(session.state.value.identity).not.toBe('');
    expect(session.state.value.identity).not.toBe(oldIdentity);
    const currentController = session.controller;
    expect(currentController).not.toBe(oldController);
    await session.refresh();
    expect(session.controller).toBe(currentController);
    expect(store.appManager).toBe(nextManager);
    expect(store.currState).toBe(nextRoot);
    currentController.navigate('secrets');
    await currentController.secrets.primary('pass-a');
    expect(currentController.secrets.state.value.cards['pass-a'].value).toEqual({
      kind: 'text',
      text: 'new owner value',
    });
    expect(nextManager.show_local_secret).toHaveBeenCalledOnce();
    expect(oldManager.show_local_secret).toHaveBeenCalledOnce();
    await oldController.confirmReset();
    expect(session.controller).toBe(currentController);
    expect(store.appManager).toBe(nextManager);
    expect(oldManager.clean_up_database).toHaveBeenCalledOnce();
    expect(nextManager.clean_up_database).not.toHaveBeenCalled();
    expect(mocks.initialize).toHaveBeenCalledTimes(2);
    for (const manager of [oldManager, nextManager]) {
      expect(manager.generate_user_creds).not.toHaveBeenCalled();
      expect(manager.sign_up).not.toHaveBeenCalled();
      expect(manager.recover_checked).not.toHaveBeenCalled();
      expect(manager.delete_secret_checked).not.toHaveBeenCalled();
      expect(manager.cluster_distribution_checked).not.toHaveBeenCalled();
      expect(manager.remove_member_checked).not.toHaveBeenCalled();
    }
    expect(platform.authenticate).not.toHaveBeenCalled();
  });
});
