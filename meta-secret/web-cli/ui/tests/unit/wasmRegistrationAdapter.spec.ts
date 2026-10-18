import { describe, expect, it, vi } from 'vitest';
import { ApplicationStateInfo, UserDataOutsiderStatus, type WasmApplicationState } from 'meta-secret-web-cli';
import { createWasmRegistrationAdapter } from '@/features/registration/wasmRegistrationAdapter';
import type { AppState } from '@/stores/app-state';

type Kind = 'local' | 'pending' | 'declined' | 'member' | 'available' | 'exists';
function makeState(kind: Kind, device = 'candidate', name = 'family'): WasmApplicationState {
  return {
    as_info: () =>
      kind === 'local'
        ? ApplicationStateInfo.Local
        : kind === 'member'
          ? ApplicationStateInfo.Member
          : kind === 'available'
            ? ApplicationStateInfo.VaultNotExists
            : ApplicationStateInfo.Outsider,
    device_id: () => ({ wasm_id_str: () => device }),
    as_vault: () => ({
      vault_name: () => name,
      as_outsider: () => ({
        status:
          kind === 'pending'
            ? UserDataOutsiderStatus.Pending
            : kind === 'declined'
              ? UserDataOutsiderStatus.Declined
              : UserDataOutsiderStatus.NonMember,
      }),
    }),
  } as unknown as WasmApplicationState;
}
function setup(initial: Kind) {
  let canonical = initial;
  let device = 'candidate';
  const calls: string[] = [];
  const store = {
    currState: makeState(initial),
    appManager: {
      sync_now: vi.fn(async () => {
        calls.push('sync');
      }),
      clean_up_database: vi.fn(async () => {
        calls.push('clean');
        canonical = 'local';
      }),
      generate_user_creds: vi.fn(async () => {
        calls.push('prepare');
        canonical = 'available';
      }),
      sign_up: vi.fn(async () => {
        calls.push('submit');
        canonical = 'pending';
        return makeState(canonical, device);
      }),
    },
    updateState: vi.fn(async () => {
      calls.push('read');
      store.currState = makeState(canonical, device);
      return store.currState;
    }),
    appStateInit: vi.fn(async () => {
      calls.push('init');
      if (canonical === 'local') device = 'fresh-bootstrap';
      store.currState = makeState(canonical, device);
    }),
    updateStateWith: (state: WasmApplicationState) => {
      store.currState = state;
    },
  };
  return {
    store,
    calls,
    canonical: (kind: Kind) => {
      canonical = kind;
    },
    port: createWasmRegistrationAdapter(store as unknown as ReturnType<typeof AppState>),
  };
}

describe('WASM registration boundary', () => {
  it('applies server decisions by syncing before reading local Core state', async () => {
    const { port, calls, canonical } = setup('pending');
    canonical('member');
    expect(await port.refresh()).toEqual({ kind: 'member', name: 'family' });
    expect(calls).toEqual(['sync', 'read']);
  });
  it('preserves a member discovered during reset preflight', async () => {
    const { port, calls, canonical, store } = setup('declined');
    canonical('member');
    await port.reset();
    expect(calls).toEqual(['sync', 'read']);
    expect(store.appManager.clean_up_database).not.toHaveBeenCalled();
  });
  it('fails closed on unknown canonical state instead of deleting credentials', async () => {
    const { port, store } = setup('declined');
    store.appManager.sync_now.mockRejectedValueOnce(new Error('offline'));
    await expect(port.reset()).rejects.toThrow('offline');
    expect(store.appManager.clean_up_database).not.toHaveBeenCalled();
  });
  it('resumes the existing pending request without a duplicate signup', async () => {
    const { port, store } = setup('pending');
    expect(await port.submit()).toEqual({ kind: 'pending', name: 'family' });
    expect(store.appManager.sign_up).not.toHaveBeenCalled();
  });
  it('only clears an unregistered attempt when selecting a different name', async () => {
    const { port, store, calls } = setup('exists');
    await port.prepare('another@example.com');
    expect(calls).toEqual(['sync', 'read', 'clean', 'init', 'prepare', 'sync', 'read']);
    expect(store.appManager.generate_user_creds).toHaveBeenCalledWith('another@example.com');
  });
  it('does not generate new credentials if a member is discovered while changing names', async () => {
    const { port, store, canonical } = setup('pending');
    canonical('member');
    expect(await port.prepare('another@example.com')).toMatchObject({ kind: 'member' });
    expect(store.appManager.clean_up_database).not.toHaveBeenCalled();
    expect(store.appManager.generate_user_creds).not.toHaveBeenCalled();
  });
});

describe('Incomplete reset authority', () => {
  it.each(['pending', 'declined'] as const)(
    'clears actual %s and verifies a new initialized Local identity',
    async (initial) => {
      const { port, calls, store } = setup(initial);
      await port.reset();
      expect(calls).toEqual(['sync', 'read', 'clean', 'init']);
      expect(port.snapshot()).toEqual({ kind: 'local', name: '' });
      expect(store.currState.device_id().wasm_id_str()).toBe('fresh-bootstrap');
    },
  );
  it.each(['exists', 'available', 'local'] as const)(
    'never clears %s through public incomplete reset',
    async (kind) => {
      const { port, store } = setup(kind);
      await expect(port.reset()).rejects.toThrow();
      expect(store.appManager.clean_up_database).not.toHaveBeenCalled();
    },
  );
  it('preserves an already known Member without entering cleanup', async () => {
    const { port, store, calls } = setup('member');
    await port.reset();
    expect(calls).toEqual([]);
    expect(store.appManager.clean_up_database).not.toHaveBeenCalled();
  });
  it('allows Pending→Declined for the exact same unfinished attempt', async () => {
    const { port, canonical, calls } = setup('pending');
    canonical('declined');
    await port.reset();
    expect(calls).toEqual(['sync', 'read', 'clean', 'init']);
  });
});

describe.each(['pending', 'declined'] as const)('%s exact cleanup guards', (initial) => {
  it('preserves a Member discovered at fresh preflight', async () => {
    const { port, canonical, store } = setup(initial);
    canonical('member');
    await port.reset();
    expect(store.appManager.clean_up_database).not.toHaveBeenCalled();
    expect(port.snapshot().kind).toBe('member');
  });
  it.each(['exists', 'available', 'local'] as const)('refuses fresh %s before delete', async (fresh) => {
    const { port, canonical, store } = setup(initial);
    canonical(fresh);
    await expect(port.reset()).rejects.toThrow();
    expect(store.appManager.clean_up_database).not.toHaveBeenCalled();
  });
  it.each(['vault', 'device', 'manager', 'null'] as const)(
    'fails closed if %s changes during preflight',
    async (changed) => {
      const { port, store } = setup(initial);
      store.updateState.mockImplementationOnce(async () => {
        if (changed === 'manager') store.appManager = { ...store.appManager };
        store.currState =
          changed === 'null'
            ? (null as unknown as WasmApplicationState)
            : makeState(
                initial,
                changed === 'device' ? 'different-device' : 'candidate',
                changed === 'vault' ? 'different-vault' : 'family',
              );
        return store.currState;
      });
      await expect(port.reset()).rejects.toThrow();
      expect(store.appManager.clean_up_database).not.toHaveBeenCalled();
    },
  );
  it('does not accept null post-init as Local proof', async () => {
    const { port, store } = setup(initial);
    store.appStateInit.mockImplementationOnce(async () => {
      store.currState = null as unknown as WasmApplicationState;
    });
    await expect(port.reset()).rejects.toThrow();
  });
  it.each(['candidate', ''] as const)('does not accept old/empty post-Local public ID (%s)', async (device) => {
    const { port, store } = setup(initial);
    store.appStateInit.mockImplementationOnce(async () => {
      store.currState = makeState('local', device);
    });
    await expect(port.reset()).rejects.toThrow();
  });
  it('refuses unreadable or null initial state without deleting', async () => {
    for (const unreadable of [
      null,
      {
        as_info: () => {
          throw new Error('unreadable');
        },
      },
    ]) {
      const { port, store } = setup(initial);
      store.currState = unreadable as unknown as WasmApplicationState;
      await expect(port.reset()).rejects.toThrow();
      expect(store.appManager.clean_up_database).not.toHaveBeenCalled();
    }
  });
  it('captures initialized manager before its async queue', async () => {
    const { port, store } = setup(initial);
    const oldManager = store.appManager;
    const resetting = port.reset();
    store.appManager = { ...oldManager };
    await expect(resetting).rejects.toThrow();
    expect(oldManager.clean_up_database).not.toHaveBeenCalled();
    expect(oldManager.sync_now).not.toHaveBeenCalled();
  });
});

it('retains the existing different-name Pending cleanup path', async () => {
  const { port, store } = setup('pending');
  await port.prepare('different@example.com');
  expect(store.appManager.clean_up_database).toHaveBeenCalledOnce();
  expect(store.appManager.generate_user_creds).toHaveBeenCalledWith('different@example.com');
});
