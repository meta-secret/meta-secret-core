import { readonly, ref } from 'vue';
import { parseEmail, parseVaultName, type RegistrationSnapshot, type RegistrationState } from './registrationState';

export interface RegistrationPort {
  snapshot(): RegistrationSnapshot;
  prepare(name: string): Promise<RegistrationSnapshot>;
  submit(): Promise<RegistrationSnapshot>;
  refresh(): Promise<RegistrationSnapshot>;
  reset(): Promise<void>;
  sync(): Promise<void>;
  observe(name: string, onResult: () => Promise<void>, onError: (message: string) => void): void;
  stopObserving(): void;
}

export function createRegistrationController(port: RegistrationPort) {
  const state = ref<RegistrationState>({
    step: 1,
    name: '',
    phase: 'choice',
    message: '',
    direction: 1,
    ready: false,
    timedCreateError: false,
  });
  let generation = 0;
  let busy = false;
  let disposed = false;
  let errorTimer: ReturnType<typeof setTimeout> | undefined;
  const current = (attempt: number) => !disposed && attempt === generation;
  const patch = (next: Partial<RegistrationState>) => {
    state.value = { ...state.value, ...next };
  };
  const fail = (message: string, timedCreateError = false) => patch({ phase: 'error', message, timedCreateError });
  const cancelTimer = () => {
    if (errorTimer) clearTimeout(errorTimer);
    errorTimer = undefined;
  };

  async function apply(snapshot: RegistrationSnapshot, attempt: number) {
    if (!current(attempt)) return;
    patch({ name: snapshot.name || state.value.name, message: '', timedCreateError: false });
    if (snapshot.kind === 'member') {
      port.stopObserving();
      patch({ phase: 'syncing' });
      try {
        await port.sync();
        if (current(attempt)) patch({ ready: true });
      } catch {
        if (current(attempt)) fail('Could not sync. Go back to retry.');
      }
    } else if (snapshot.kind === 'pending') {
      patch({ step: 4, phase: 'pending' });
      port.observe(
        state.value.name,
        async () => {
          if (!current(attempt)) return;
          try {
            await apply(await port.refresh(), attempt);
          } catch {
            if (current(attempt)) {
              port.stopObserving();
              fail('Network error. Go back to retry.');
            }
          }
        },
        () => {
          if (current(attempt)) {
            port.stopObserving();
            fail('Live updates unavailable. Go back to retry.');
          }
        },
      );
    } else if (snapshot.kind === 'declined') {
      port.stopObserving();
      patch({ step: 4 });
      fail('Request declined.');
    } else if (snapshot.kind === 'available' || snapshot.kind === 'exists') {
      patch({ step: 2, phase: snapshot.kind });
    } else patch({ step: 1, phase: 'choice' });
  }

  async function chooseName(input: string) {
    if (busy || disposed) return;
    let name: string;
    try {
      name = parseVaultName(input);
    } catch (error) {
      fail((error as Error).message);
      return;
    }
    busy = true;
    const attempt = ++generation;
    cancelTimer();
    port.stopObserving();
    patch({ step: 2, name, phase: 'checking', message: '', direction: 1, timedCreateError: false });
    try {
      const snapshot = await port.prepare(name);
      await apply(
        snapshot.kind === 'pending' || snapshot.kind === 'declined' ? { ...snapshot, kind: 'exists' } : snapshot,
        attempt,
      );
    } catch {
      if (current(attempt)) fail('Network error. Go back to retry.');
    } finally {
      if (current(attempt)) busy = false;
    }
  }

  async function submit(mode: 'create' | 'join') {
    if (busy || disposed) return;
    if (mode === 'create' && state.value.phase !== 'available') return;
    if (mode === 'join' && state.value.phase !== 'join') return;
    busy = true;
    const attempt = generation;
    patch({ phase: mode === 'create' ? 'creating' : 'requesting', message: '' });
    try {
      await apply(await port.submit(), attempt);
    } catch (error) {
      if (!current(attempt)) return;
      const message = String(error);
      if (message.toLowerCase().includes('at most 3 devices')) {
        fail('This Vault already has 3 devices.');
      } else {
        try {
          const snapshot = await port.refresh();
          if (!current(attempt)) return;
          if (snapshot.kind === 'member' || snapshot.kind === 'pending' || snapshot.kind === 'declined') {
            await apply(snapshot, attempt);
          } else if (snapshot.kind === 'available' || snapshot.kind === 'exists') {
            fail('Network error. Try again.', mode === 'create');
            if (mode === 'create')
              errorTimer = setTimeout(() => {
                if (current(attempt))
                  patch({
                    step: 2,
                    phase: snapshot.kind === 'exists' ? 'exists' : 'available',
                    message: '',
                    timedCreateError: false,
                  });
              }, 3000);
          } else fail('Result unknown. Go back to retry.');
        } catch {
          if (current(attempt)) fail('Result unknown. Go back to retry.');
        }
      }
    } finally {
      if (current(attempt)) busy = false;
    }
  }

  async function back() {
    if (busy || disposed || state.value.timedCreateError) return;
    cancelTimer();
    if (state.value.step === 3 || state.value.phase === 'join') {
      patch({
        step: state.value.name ? 2 : 1,
        phase: state.value.name ? (port.snapshot().kind === 'available' ? 'available' : 'exists') : 'choice',
        message: '',
        direction: -1,
      });
      return;
    }
    const attempt = ++generation;
    port.stopObserving();
    let snapshot: RegistrationSnapshot;
    try {
      snapshot = port.snapshot();
    } catch {
      fail('Could not read Vault state. Go back to retry.');
      return;
    }
    if (snapshot.kind === 'member') {
      busy = true;
      try {
        await apply(snapshot, attempt);
      } finally {
        busy = false;
      }
      return;
    }
    if (snapshot.kind === 'pending' || snapshot.kind === 'declined') {
      busy = true;
      try {
        await port.reset();
        if (!current(attempt)) return;
        const after = port.snapshot();
        if (after.kind === 'member') await apply(after, attempt);
        else if (after.kind === 'local' && after.name === '')
          patch({
            step: 1,
            phase: 'choice',
            name: '',
            message: '',
            direction: -1,
            ready: false,
            timedCreateError: false,
          });
        else fail('Could not reset. Go back to retry.');
      } catch {
        if (current(attempt)) fail('Could not reset. Go back to retry.');
      } finally {
        if (current(attempt)) busy = false;
      }
      return;
    }
    // Other Back actions leave the current presentation without clearing credentials.
    patch({ step: 2, phase: snapshot.kind === 'available' ? 'available' : 'exists', message: '', direction: -1 });
  }

  return {
    state: readonly(state),
    chooseName,
    chooseEmail: async (input: string) => {
      try {
        await chooseName(parseEmail(input));
      } catch {
        fail('Enter a valid email.');
      }
    },
    resume: async () => apply(port.snapshot(), generation),
    create: () => submit('create'),
    request: () => submit('join'),
    join: () => {
      if (!busy && !disposed && state.value.step === 2 && state.value.phase === 'exists')
        patch({ step: 4, phase: 'join', message: '', direction: 1 });
    },
    edit: () => {
      if (busy || disposed || state.value.timedCreateError) return;
      cancelTimer();
      patch({ step: 3, phase: 'editing', message: '', direction: 1, timedCreateError: false });
    },
    back,
    providerError: (message: string) => fail(message),
    scan: async (name: string) => {
      await chooseName(name);
      if (state.value.phase === 'exists') {
        patch({ step: 4, phase: 'join' });
        await submit('join');
      } else if (state.value.phase === 'available') {
        patch({ step: 4 });
        fail('This Vault does not exist.');
      }
    },
    dispose: () => {
      disposed = true;
      generation += 1;
      cancelTimer();
      port.stopObserving();
    },
  };
}
