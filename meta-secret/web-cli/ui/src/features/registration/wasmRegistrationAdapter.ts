import { ApplicationStateInfo, UserDataOutsiderStatus, type WasmApplicationState } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { createJoinResultController } from '@/utils/joinResult';
import type { RegistrationPort } from './registrationController';
import type { RegistrationSnapshot } from './registrationState';

export function readRegistrationSnapshot(state: WasmApplicationState | null): RegistrationSnapshot {
  if (!state || state.as_info() === ApplicationStateInfo.Local) return { kind: 'local', name: '' };
  const name = state.as_vault().vault_name();
  if (state.as_info() === ApplicationStateInfo.Member) return { kind: 'member', name };
  if (state.as_info() === ApplicationStateInfo.VaultNotExists) return { kind: 'available', name };
  const status = state.as_vault().as_outsider().status;
  return {
    kind:
      status === UserDataOutsiderStatus.Pending
        ? 'pending'
        : status === UserDataOutsiderStatus.Declined
          ? 'declined'
          : 'exists',
    name,
  };
}

export function createWasmRegistrationAdapter(store = AppState()): RegistrationPort {
  let operations: Promise<unknown> = Promise.resolve();
  let stream: ReturnType<typeof createJoinResultController> | undefined;
  const serialize = <T>(action: () => Promise<T>): Promise<T> => {
    const next = operations.then(action, action);
    operations = next.catch(() => undefined);
    return next;
  };
  const manager = () => {
    if (!store.appManager) throw new Error('Vault is not initialized.');
    return store.appManager;
  };
  const snapshot = () => readRegistrationSnapshot(store.currState);
  let stopLifecycle: (() => void) | undefined;
  const stopObserving = () => {
    stream?.disconnect();
    stream = undefined;
    stopLifecycle?.();
    stopLifecycle = undefined;
  };
  const reset = async () => {
    if (snapshot().kind !== 'local') {
      await manager().sync_now();
      await store.updateState();
    }
    if (snapshot().kind === 'member') return;
    await manager().clean_up_database();
    await store.appStateInit();
    if (snapshot().kind !== 'local') throw new Error('Local reset did not complete.');
  };
  const resetIncomplete = async () => {
    const capturedManager = manager();
    const capturedState = store.currState;
    if (!capturedState) throw new Error('Canonical registration state is unavailable.');
    const captured = readRegistrationSnapshot(capturedState);
    if (captured.kind === 'member') return;
    if ((captured.kind !== 'pending' && captured.kind !== 'declined') || !captured.name)
      throw new Error('Only an incomplete attempt can be reset.');
    const capturedDevice = capturedState.device_id().wasm_id_str();
    if (!capturedDevice) throw new Error('Canonical device identity is unavailable.');
    await serialize(async () => {
      if (store.appManager !== capturedManager) throw new Error('Registration context changed.');
      await capturedManager.sync_now();
      if (store.appManager !== capturedManager) throw new Error('Registration context changed.');
      await store.updateState();
      const freshState = store.currState;
      if (!freshState) throw new Error('Canonical registration state is unavailable.');
      const fresh = readRegistrationSnapshot(freshState);
      if (fresh.kind === 'member') return;
      if (
        store.appManager !== capturedManager ||
        (fresh.kind !== 'pending' && fresh.kind !== 'declined') ||
        fresh.name !== captured.name ||
        freshState.device_id().wasm_id_str() !== capturedDevice
      )
        throw new Error('Incomplete registration identity changed.');
      await capturedManager.clean_up_database();
      if (store.appManager !== capturedManager) throw new Error('Registration context changed during cleanup.');
      await store.appStateInit();
      if (!store.appManager || !store.currState) throw new Error('Local reset state is unavailable.');
      const after = readRegistrationSnapshot(store.currState);
      if (after.kind !== 'local' && after.kind !== 'member') throw new Error('Local reset did not complete.');
      if (after.kind === 'local') {
        const nextDevice = store.currState.device_id().wasm_id_str();
        if (!nextDevice || nextDevice === capturedDevice) throw new Error('Local reset identity did not change.');
      }
    });
  };
  return {
    snapshot,
    prepare: (name) =>
      serialize(async () => {
        const before = snapshot();
        if (before.kind === 'member') return before;
        if (before.name === name && before.kind !== 'local') {
          await manager().sync_now();
          await store.updateState();
          return snapshot();
        }
        if (before.kind !== 'local') await reset();
        if (snapshot().kind === 'member') return snapshot();
        await manager().generate_user_creds(name);
        await manager().sync_now();
        await store.updateState();
        return snapshot();
      }),
    submit: () =>
      serialize(async () => {
        await manager().sync_now();
        await store.updateState();
        const current = snapshot();
        if (current.kind === 'pending' || current.kind === 'member') return current;
        const next = await manager().sign_up();
        store.updateStateWith(next);
        return snapshot();
      }),
    refresh: () =>
      serialize(async () => {
        if (snapshot().kind !== 'local') await manager().sync_now();
        await store.updateState();
        return snapshot();
      }),
    reset: resetIncomplete,
    sync: () =>
      serialize(async () => {
        await manager().sync_now();
        await store.updateState();
        if (snapshot().kind !== 'member') throw new Error('Vault connection is not ready.');
      }),
    observe: (name, onResult, onError) => {
      stopObserving();
      stream = createJoinResultController({
        refresh: onResult,
        getAuthorization: () => store.getJoinResultAuthToken(),
        onError: (_message, terminal) => {
          if (terminal) onError('Live approval updates unavailable.');
        },
      });
      const reconnect = () => {
        if (document.visibilityState !== 'visible' || snapshot().kind !== 'pending') return;
        stream?.disconnect();
        stream?.connect(name);
        void onResult();
      };
      document.addEventListener('visibilitychange', reconnect);
      window.addEventListener('online', reconnect);
      stopLifecycle = () => {
        document.removeEventListener('visibilitychange', reconnect);
        window.removeEventListener('online', reconnect);
      };
      stream.connect(name);
    },
    stopObserving,
  };
}
