import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { reactive } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import PasskeyAuth from '@/components/PasskeyAuth.vue';

type AuthState = {
  isAuthenticated: boolean;
  hasRegisteredPasskey: boolean;
  isWebAuthnSupported: boolean;
  createPasskeyCredential: ReturnType<typeof vi.fn<() => Promise<boolean>>>;
  authenticateWithPasskey: ReturnType<typeof vi.fn<() => Promise<boolean>>>;
};

const mocks = vi.hoisted(() => ({ store: undefined as unknown as AuthState }));
vi.mock('@/stores/auth', () => ({ useAuthStore: () => mocks.store }));

let wrapper: VueWrapper | undefined;
function render() {
  wrapper = mount(PasskeyAuth, {
    global: {
      stubs: {
        Dialog: { props: ['open'], template: '<div v-if="open" data-testid="passkey-dialog"><slot /></div>' },
        DialogContent: { template: '<section><slot /></section>' },
        DialogHeader: { template: '<header><slot /></header>' },
        DialogTitle: { template: '<h2><slot /></h2>' },
        DialogDescription: { template: '<p data-testid="description"><slot /></p>' },
        Button: { props: ['disabled'], template: '<button :disabled="disabled"><slot /></button>' },
      },
    },
  });
  return wrapper;
}

beforeEach(() => {
  vi.useFakeTimers();
  mocks.store = reactive({
    isAuthenticated: false,
    hasRegisteredPasskey: false,
    isWebAuthnSupported: true,
    createPasskeyCredential: vi.fn<() => Promise<boolean>>().mockResolvedValue(false),
    authenticateWithPasskey: vi.fn<() => Promise<boolean>>().mockResolvedValue(false),
  });
});
afterEach(() => {
  wrapper?.unmount();
  wrapper = undefined;
  vi.clearAllTimers();
  vi.useRealTimers();
});

describe('Passkey dialog presentation with existing authentication actions', () => {
  it('explains browser setup without claiming an existing Vault or biometric-only verification', () => {
    const view = render();
    expect(view.get('h2').text()).toBe('Set up MetaSecret');
    expect(view.get('[data-testid="description"]').text()).toBe('Create a passkey to get started on this browser.');
    expect(view.get('button').text()).toBe('Create Passkey');
    expect(view.get('img[alt="MetaSecret"]').attributes('src')).toMatch(
      /^(?:\/metasecret-gray\.svg|data:image\/svg\+xml,)/,
    );
    expect(view.find('svg').exists()).toBe(false);
  });

  it('explains unlocking an existing browser credential', () => {
    mocks.store.hasRegisteredPasskey = true;
    const view = render();
    expect(view.get('h2').text()).toBe('Unlock MetaSecret');
    expect(view.get('[data-testid="description"]').text()).toBe('Use your passkey to continue on this browser.');
    expect(view.get('button').text()).toBe('Authenticate with Passkey');
  });

  it.each([false, true])('never starts a credential operation on mount (registered=%s)', async (registered) => {
    mocks.store.hasRegisteredPasskey = registered;
    render();
    await flushPromises();
    expect(mocks.store.createPasskeyCredential).not.toHaveBeenCalled();
    expect(mocks.store.authenticateWithPasskey).not.toHaveBeenCalled();
  });

  it('keeps unsupported browser setup disabled and explains the existing limitation', () => {
    mocks.store.isWebAuthnSupported = false;
    const view = render();
    expect((view.get('button').element as HTMLButtonElement).disabled).toBe(true);
    expect(view.text()).toContain('Your browser does not support passkeys.');
    expect(mocks.store.createPasskeyCredential).not.toHaveBeenCalled();
  });

  it('keeps creation failure visible without trying to authenticate', async () => {
    const view = render();
    await view.get('button').trigger('click');
    await flushPromises();
    expect(mocks.store.createPasskeyCredential).toHaveBeenCalledOnce();
    expect(mocks.store.authenticateWithPasskey).not.toHaveBeenCalled();
    expect(view.text()).toContain('Failed to create passkey');
  });

  it('keeps a thrown creation error visible and restores the action', async () => {
    mocks.store.createPasskeyCredential.mockRejectedValue(new Error('Creation cancelled'));
    const view = render();
    await view.get('button').trigger('click');
    await flushPromises();
    expect(view.text()).toContain('Creation cancelled');
    expect((view.get('button').element as HTMLButtonElement).disabled).toBe(false);
  });

  it('preserves the existing create then authenticate call sequence', async () => {
    const calls: string[] = [];
    mocks.store.createPasskeyCredential.mockImplementation(async () => {
      calls.push('create');
      return true;
    });
    mocks.store.authenticateWithPasskey.mockImplementation(async () => {
      calls.push('authenticate');
      return true;
    });
    const view = render();
    await view.get('button').trigger('click');
    await flushPromises();
    expect(calls).toEqual(['create', 'authenticate']);
    expect(view.text()).toContain('Authentication successful');
  });

  it('keeps a failed unlock visible without creating another credential', async () => {
    mocks.store.hasRegisteredPasskey = true;
    const view = render();
    await view.get('button').trigger('click');
    await flushPromises();
    expect(mocks.store.authenticateWithPasskey).toHaveBeenCalledOnce();
    expect(mocks.store.createPasskeyCredential).not.toHaveBeenCalled();
    expect(view.text()).toContain('Authentication failed');
  });

  it('keeps a thrown unlock error visible and restores the action', async () => {
    mocks.store.hasRegisteredPasskey = true;
    mocks.store.authenticateWithPasskey.mockRejectedValue(new Error('Unlock cancelled'));
    const view = render();
    await view.get('button').trigger('click');
    await flushPromises();
    expect(view.text()).toContain('Unlock cancelled');
    expect((view.get('button').element as HTMLButtonElement).disabled).toBe(false);
  });

  it('keeps creation disabled and its existing progress label while pending', async () => {
    let finish!: (success: boolean) => void;
    mocks.store.createPasskeyCredential.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const view = render();
    await view.get('button').trigger('click');
    expect((view.get('button').element as HTMLButtonElement).disabled).toBe(true);
    expect(view.get('button').text()).toBe('Creating Passkey...');
    finish(false);
    await flushPromises();
    expect(view.get('button').text()).toBe('Create Passkey');
  });

  it('keeps unlock disabled and clears the previous error on an explicit retry', async () => {
    mocks.store.hasRegisteredPasskey = true;
    const view = render();
    await view.get('button').trigger('click');
    await flushPromises();
    expect(view.text()).toContain('Authentication failed');
    let finish!: (success: boolean) => void;
    mocks.store.authenticateWithPasskey.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await view.get('button').trigger('click');
    expect((view.get('button').element as HTMLButtonElement).disabled).toBe(true);
    expect(view.get('button').text()).toBe('Authenticating...');
    expect(view.text()).not.toContain('Authentication failed');
    finish(false);
    await flushPromises();
  });

  it('hides only when the existing auth store reports authentication', async () => {
    const view = render();
    expect(view.find('[data-testid="passkey-dialog"]').exists()).toBe(true);
    mocks.store.isAuthenticated = true;
    await flushPromises();
    expect(view.find('[data-testid="passkey-dialog"]').exists()).toBe(false);
    expect(mocks.store.createPasskeyCredential).not.toHaveBeenCalled();
    expect(mocks.store.authenticateWithPasskey).not.toHaveBeenCalled();
  });
});
