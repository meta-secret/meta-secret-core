import { describe, expect, it, vi } from 'vitest';
import { createRegistrationController, type RegistrationPort } from '@/features/registration/registrationController';

function setup(overrides: Partial<RegistrationPort> = {}) {
  const port: RegistrationPort = {
    snapshot: () => ({ kind: 'local', name: '' }),
    prepare: vi.fn().mockResolvedValue({ kind: 'available', name: 'person@example.com' }),
    submit: vi.fn().mockResolvedValue({ kind: 'pending', name: 'person@example.com' }),
    refresh: vi.fn().mockResolvedValue({ kind: 'available', name: 'person@example.com' }),
    reset: vi.fn().mockResolvedValue(undefined),
    sync: vi.fn().mockResolvedValue(undefined),
    observe: vi.fn(),
    stopObserving: vi.fn(),
    ...overrides,
  };
  const controller = createRegistrationController(port);
  return { port, controller };
}

describe('Orbit registration coordination', () => {
  it('prepares availability without submitting and disables Create for a taken name', async () => {
    const { port, controller } = setup({
      prepare: vi.fn().mockResolvedValue({ kind: 'exists', name: 'person@example.com' }),
    });
    await controller.chooseName(' person@example.com ');
    expect(controller.state.value).toMatchObject({ step: 2, name: 'person@example.com', phase: 'exists' });
    await controller.create();
    expect(port.submit).not.toHaveBeenCalled();
  });
  it('submits a single request and waits indefinitely for canonical membership', async () => {
    let finish!: (value: { kind: 'pending'; name: string }) => void;
    const { port, controller } = setup({
      prepare: vi.fn().mockResolvedValue({ kind: 'exists', name: 'person@example.com' }),
      submit: vi.fn(
        () =>
          new Promise<{ kind: 'pending'; name: string }>((resolve) => {
            finish = resolve;
          }),
      ),
    });
    await controller.chooseName('person@example.com');
    controller.join();
    const request = controller.request();
    await controller.request();
    expect(port.submit).toHaveBeenCalledTimes(1);
    finish({ kind: 'pending', name: 'person@example.com' });
    await request;
    expect(controller.state.value).toMatchObject({ step: 4, phase: 'pending', name: 'person@example.com' });
    expect(port.sync).not.toHaveBeenCalled();
  });
  it.each(['pending', 'declined'] as const)(
    'retains %s on04 until Back then clears the attempt and opens01',
    async (initial) => {
      let kind: 'pending' | 'declined' | 'local' = initial;
      const { port, controller } = setup({
        snapshot: () => ({ kind, name: kind === 'local' ? '' : 'person@example.com' }),
        reset: vi.fn(async () => {
          kind = 'local';
        }),
      });
      await controller.resume();
      expect(controller.state.value).toMatchObject({ step: 4, phase: initial === 'declined' ? 'error' : 'pending' });
      await controller.back();
      expect(port.stopObserving).toHaveBeenCalled();
      expect(port.reset).toHaveBeenCalledOnce();
      expect(controller.state.value).toMatchObject({ step: 1, phase: 'choice', name: '', direction: -1 });
    },
  );
  it('rejects stale observation callbacks after a local reset', async () => {
    let cleared = false;
    let observed!: () => Promise<void>;
    const { controller } = setup({
      snapshot: () => (cleared ? { kind: 'local', name: '' } : { kind: 'pending', name: 'person@example.com' }),
      reset: async () => {
        cleared = true;
      },
      observe: (_name, onResult) => {
        observed = onResult;
      },
    });
    await controller.resume();
    await controller.back();
    await observed();
    expect(controller.state.value).toMatchObject({ step: 1, phase: 'choice' });
  });
  it('requires successful Core sync before readiness and never adds authentication', async () => {
    let finishSync!: () => void;
    const { controller } = setup({
      submit: vi.fn().mockResolvedValue({ kind: 'member', name: 'person@example.com' }),
      sync: () =>
        new Promise((resolve) => {
          finishSync = resolve;
        }),
    });
    await controller.chooseName('person@example.com');
    const create = controller.create();
    await vi.waitFor(() => expect(controller.state.value.phase).toBe('syncing'));
    expect(controller.state.value.ready).toBe(false);
    finishSync();
    await create;
    expect(controller.state.value.ready).toBe(true);
  });
  it('reconciles an unknown submit outcome instead of submitting twice', async () => {
    const { port, controller } = setup({
      submit: vi.fn().mockRejectedValue(new TypeError('Failed to fetch')),
      refresh: vi.fn().mockResolvedValue({ kind: 'pending', name: 'person@example.com' }),
    });
    await controller.chooseName('person@example.com');
    await controller.create();
    expect(port.refresh).toHaveBeenCalledOnce();
    expect(controller.state.value.phase).toBe('pending');
    expect(port.submit).toHaveBeenCalledOnce();
  });
  it('does not clear local data if a late result already made this device a member', async () => {
    let kind: 'pending' | 'member' = 'pending';
    const { port, controller } = setup({ snapshot: () => ({ kind, name: 'person@example.com' }) });
    await controller.resume();
    kind = 'member';
    await controller.back();
    expect(port.reset).not.toHaveBeenCalled();
    expect(controller.state.value.ready).toBe(true);
  });
});

describe('QR and error recovery', () => {
  it('uses a plain CLI Vault name and immediately submits the ordinary request', async () => {
    const { port, controller } = setup({
      prepare: vi.fn().mockResolvedValue({ kind: 'exists', name: 'Family Vault' }),
    });
    await controller.scan('Family Vault');
    expect(port.prepare).toHaveBeenCalledWith('Family Vault');
    expect(port.submit).toHaveBeenCalledOnce();
    expect(controller.state.value.phase).toBe('pending');
  });
  it('does not create an unknown scanned name', async () => {
    const { port, controller } = setup();
    await controller.scan('missing');
    expect(port.submit).not.toHaveBeenCalled();
    expect(controller.state.value).toMatchObject({ step: 4, phase: 'error', message: 'This Vault does not exist.' });
  });
  it('keeps a known Create failure visible for3seconds then permits another deliberate action', async () => {
    vi.useFakeTimers();
    try {
      const { controller } = setup({
        submit: vi.fn().mockRejectedValue(new Error('network')),
        refresh: vi.fn().mockResolvedValue({ kind: 'available', name: 'person@example.com' }),
      });
      await controller.chooseName('person@example.com');
      await controller.create();
      expect(controller.state.value).toMatchObject({ phase: 'error', message: 'Network error. Try again.' });
      await vi.advanceTimersByTimeAsync(2999);
      expect(controller.state.value.phase).toBe('error');
      await vi.advanceTimersByTimeAsync(1);
      expect(controller.state.value.phase).toBe('available');
    } finally {
      vi.useRealTimers();
    }
  });
  it('keeps Join network errors on04 until Back', async () => {
    const { controller } = setup({
      prepare: vi.fn().mockResolvedValue({ kind: 'exists', name: 'person@example.com' }),
      submit: vi.fn().mockRejectedValue(new Error('network')),
      refresh: vi.fn().mockResolvedValue({ kind: 'exists', name: 'person@example.com' }),
    });
    await controller.chooseName('person@example.com');
    controller.join();
    await controller.request();
    expect(controller.state.value).toMatchObject({ step: 4, phase: 'error', message: 'Network error. Try again.' });
  });
  it('keeps Pending04 actionable if canonical cleanup cannot complete', async () => {
    const { port, controller } = setup({
      snapshot: () => ({ kind: 'pending', name: 'person@example.com' }),
      refresh: vi.fn().mockRejectedValue(new Error('offline')),
      reset: vi.fn().mockRejectedValue(new Error('offline')),
    });
    await controller.resume();
    await controller.back();
    expect(controller.state.value).toMatchObject({ step: 4, phase: 'error', name: 'person@example.com' });
    expect(port.reset).toHaveBeenCalledOnce();
    expect(port.refresh).not.toHaveBeenCalled();
  });
});

describe('Join availability intent gate', () => {
  it('rejects forced Join for a Core-derived available name without changing state or submitting', async () => {
    const { controller, port } = setup();
    await controller.chooseName('person@example.com');
    const before = { ...controller.state.value };
    controller.join();
    await controller.request();
    expect(controller.state.value).toEqual(before);
    expect(port.submit).not.toHaveBeenCalled();
  });

  it('permits only an existing Vault to enter Join and submit on deliberate Request', async () => {
    const { controller, port } = setup({
      prepare: vi.fn().mockResolvedValue({ kind: 'exists', name: 'person@example.com' }),
    });
    await controller.chooseName('person@example.com');
    controller.join();
    expect(controller.state.value).toMatchObject({ step: 4, phase: 'join', direction: 1 });
    expect(port.submit).not.toHaveBeenCalled();
    await controller.request();
    expect(port.submit).toHaveBeenCalledOnce();
  });

  it('does not navigate while checking and remains blocked when that check returns available', async () => {
    let finish!: (value: { kind: 'available'; name: string }) => void;
    const { controller, port } = setup({
      prepare: () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    });
    const checking = controller.chooseName('person@example.com');
    controller.join();
    expect(controller.state.value).toMatchObject({ step: 2, phase: 'checking' });
    finish({ kind: 'available', name: 'person@example.com' });
    await checking;
    controller.join();
    expect(controller.state.value).toMatchObject({ step: 2, phase: 'available' });
    expect(port.submit).not.toHaveBeenCalled();
  });

  it('does not activate Join from an error or a disposed existing-name controller', async () => {
    const failed = setup({ prepare: vi.fn().mockRejectedValue(new Error('offline')) });
    await failed.controller.chooseName('person@example.com');
    failed.controller.join();
    expect(failed.controller.state.value).toMatchObject({ step: 2, phase: 'error' });
    const existing = setup({ prepare: vi.fn().mockResolvedValue({ kind: 'exists', name: 'person@example.com' }) });
    await existing.controller.chooseName('person@example.com');
    existing.controller.dispose();
    existing.controller.join();
    expect(existing.controller.state.value).toMatchObject({ step: 2, phase: 'exists' });
    expect(existing.port.submit).not.toHaveBeenCalled();
  });
});

describe('Uncertain outcomes and navigation direction', () => {
  it('does not turn a missing/unknown reconciliation into an available name', async () => {
    vi.useFakeTimers();
    try {
      const { controller } = setup({
        submit: vi.fn().mockRejectedValue(new Error('network')),
        refresh: vi.fn().mockResolvedValue({ kind: 'local', name: '' }),
      });
      await controller.chooseName('person@example.com');
      await controller.create();
      await vi.advanceTimersByTimeAsync(4000);
      expect(controller.state.value).toMatchObject({ phase: 'error', message: 'Result unknown. Go back to retry.' });
    } finally {
      vi.useRealTimers();
    }
  });
  it('rotates OK clockwise and Back counterclockwise', async () => {
    const { controller } = setup();
    controller.edit();
    await controller.chooseEmail('person@example.com');
    expect(controller.state.value).toMatchObject({ step: 2, direction: 1 });
    controller.edit();
    await controller.back();
    expect(controller.state.value).toMatchObject({ step: 2, direction: -1 });
  });
});

describe('Availability and stream boundaries', () => {
  it('returns to02 after manual OK even when Core already has a pending request', async () => {
    const { port, controller } = setup({
      prepare: vi.fn().mockResolvedValue({ kind: 'pending', name: 'person@example.com' }),
    });
    controller.edit();
    await controller.chooseEmail('person@example.com');
    expect(controller.state.value).toMatchObject({ step: 2, phase: 'exists' });
    expect(port.submit).not.toHaveBeenCalled();
    expect(port.observe).not.toHaveBeenCalled();
  });
  it('stops observation on a terminal live-update failure so the error stays until Back', async () => {
    let failure!: (message: string) => void;
    const { port, controller } = setup({
      snapshot: () => ({ kind: 'pending', name: 'person@example.com' }),
      observe: (_name, _result, onError) => {
        failure = onError;
      },
    });
    await controller.resume();
    failure('unauthorized');
    expect(port.stopObserving).toHaveBeenCalledOnce();
    expect(controller.state.value).toMatchObject({ step: 4, phase: 'error' });
  });
});

describe('Occupied-name invariant after Back', () => {
  it.each(['pending'] as const)('never enables Create for %s when going03→Back', async (kind) => {
    const { port, controller } = setup({
      snapshot: () => ({ kind, name: 'person@example.com' }),
      prepare: vi.fn().mockResolvedValue({ kind, name: 'person@example.com' }),
    });
    await controller.chooseName('person@example.com');
    controller.edit();
    await controller.back();
    expect(controller.state.value).toMatchObject({ step: 2, phase: 'exists' });
    await controller.create();
    expect(port.submit).not.toHaveBeenCalled();
  });
});

describe('Timed Create error navigation', () => {
  it('does not expose manual editing during a timed error and cancels the timer on disposal', async () => {
    vi.useFakeTimers();
    try {
      const { controller } = setup({ submit: vi.fn().mockRejectedValue(new Error('network')) });
      await controller.chooseName('person@example.com');
      await controller.create();
      controller.edit();
      expect(controller.state.value).toMatchObject({ step: 2, phase: 'error' });
      controller.dispose();
      await vi.advanceTimersByTimeAsync(4000);
      expect(controller.state.value.phase).toBe('error');
    } finally {
      vi.useRealTimers();
    }
  });
});

describe('Incomplete local cleanup', () => {
  it.each(['pending', 'declined'] as const)('waits for %s cleanup and serializes duplicate Back', async (initial) => {
    let kind: 'pending' | 'declined' | 'local' = initial;
    let finish!: () => void;
    const { port, controller } = setup({
      snapshot: () => ({ kind, name: kind === 'local' ? '' : 'person@example.com' }),
      reset: vi.fn(
        () =>
          new Promise<void>((resolve) => {
            finish = () => {
              kind = 'local';
              resolve();
            };
          }),
      ),
    });
    await controller.resume();
    const first = controller.back();
    await controller.back();
    expect(port.reset).toHaveBeenCalledOnce();
    expect(controller.state.value.step).toBe(4);
    finish();
    await first;
    expect(controller.state.value).toMatchObject({ step: 1, name: '', phase: 'choice' });
  });
  it.each(['pending', 'declined'] as const)(
    'does not claim01 after %s cleanup failure or unexpected result',
    async (initial) => {
      for (const reset of [
        vi.fn().mockRejectedValue(new Error('cleanup failed')),
        vi.fn().mockResolvedValue(undefined),
      ]) {
        const { controller } = setup({ snapshot: () => ({ kind: initial, name: 'person@example.com' }), reset });
        await controller.resume();
        await controller.back();
        expect(controller.state.value).toMatchObject({ step: 4, name: 'person@example.com', phase: 'error' });
        expect(controller.state.value.message).toContain('reset');
      }
    },
  );
  it.each(['pending', 'declined'] as const)(
    'preserves Member discovered during %s reset instead of showing01',
    async (initial) => {
      let kind: 'pending' | 'declined' | 'member' = initial;
      const { port, controller } = setup({
        snapshot: () => ({ kind, name: 'person@example.com' }),
        reset: vi.fn(async () => {
          kind = 'member';
        }),
      });
      await controller.resume();
      await controller.back();
      expect(controller.state.value.ready).toBe(true);
      expect(controller.state.value.step).not.toBe(1);
      expect(port.sync).toHaveBeenCalledOnce();
    },
  );
  it.each(['pending', 'declined'] as const)(
    'invalidates an already in-flight observer before %s reset/new01',
    async (cleanupKind) => {
      let kind: 'pending' | 'declined' | 'local' = 'pending';
      let observe!: () => Promise<void>;
      let finishRefresh!: (value: { kind: 'pending'; name: string }) => void;
      const { controller } = setup({
        snapshot: () => ({ kind, name: kind === 'local' ? '' : 'person@example.com' }),
        observe: (_name, callback) => {
          observe = callback;
        },
        refresh: () =>
          new Promise((resolve) => {
            finishRefresh = resolve;
          }),
        reset: async () => {
          kind = 'local';
        },
      });
      await controller.resume();
      const stale = observe();
      kind = cleanupKind;
      if (cleanupKind === 'declined') await controller.resume();
      await controller.back();
      finishRefresh({ kind: 'pending', name: 'person@example.com' });
      await stale;
      expect(controller.state.value).toMatchObject({ step: 1, name: '', phase: 'choice' });
    },
  );
  it('keeps pre-request and manual Back nondestructive', async () => {
    const { port, controller } = setup({
      snapshot: () => ({ kind: 'exists', name: 'person@example.com' }),
      prepare: vi.fn().mockResolvedValue({ kind: 'exists', name: 'person@example.com' }),
    });
    await controller.chooseName('person@example.com');
    expect(controller.state.value).toMatchObject({ step: 2, name: 'person@example.com' });
    controller.join();
    await controller.back();
    expect(controller.state.value.step).toBe(2);
    controller.edit();
    await controller.back();
    expect(controller.state.value.step).toBe(2);
    expect(port.reset).not.toHaveBeenCalled();
  });
});
