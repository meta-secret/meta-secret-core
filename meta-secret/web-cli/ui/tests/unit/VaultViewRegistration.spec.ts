import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TransportSk } from 'meta-secret-web-cli';
import App from '@/App.vue';
import router from '@/router';
import { AppState } from '@/stores/app-state';
import { registrationSurfaceActive } from '@/features/registration/registrationSurface';
import { managerFixture, stateFixture } from './liveboardCoreFixtures';

const mocks = vi.hoisted(() => ({ initialize: vi.fn() }));
vi.mock('meta-secret-web-cli', async (importOriginal) => ({
  ...(await importOriginal<typeof import('meta-secret-web-cli')>()),
  default: vi.fn(async () => {}),
  MasterKeyManager: {
    from_pure_sk: vi.fn(
      (): TransportSk => ({
        free: vi.fn(),
        [Symbol.dispose]: vi.fn(),
        0: { free: vi.fn(), [Symbol.dispose]: vi.fn() },
      }),
    ),
  },
  WasmApplicationManager: { init_wasm_with_device_and_environment: mocks.initialize },
}));
vi.mock('@/stores/auth', () => ({ useAuthStore: () => ({ isAuthenticated: true, masterKey: 'test-key' }) }));
vi.mock('@/stores/theme', () => ({ useThemeStore: vi.fn() }));
vi.mock('@/utils/stateInvalidation', () => ({
  createStateInvalidationController: () => ({ connect: vi.fn(), disconnect: vi.fn(), refreshNow: vi.fn() }),
}));
vi.mock('@/components/vault/Vault.vue', () => ({
  default: { template: '<div data-testid="legacy-vault">Legacy Vault</div>' },
}));
vi.mock('@/components/registration/OrbitRegistration.vue', () => ({
  default: { template: '<div data-testid="orbit-entry">Orbit registration</div>' },
}));

const showModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'showModal');
const close = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'close');
let wrapper: VueWrapper | undefined;
beforeEach(() => {
  registrationSurfaceActive.value = false;
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: vi.fn(), removeEventListener: vi.fn() }));
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true,
    value(this: HTMLDialogElement) {
      this.open = true;
    },
  });
  Object.defineProperty(HTMLDialogElement.prototype, 'close', {
    configurable: true,
    value(this: HTMLDialogElement) {
      this.open = false;
    },
  });
});
afterEach(() => {
  wrapper?.unmount();
  wrapper = undefined;
  registrationSurfaceActive.value = false;
  vi.unstubAllGlobals();
  if (showModal) Object.defineProperty(HTMLDialogElement.prototype, 'showModal', showModal);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'showModal');
  if (close) Object.defineProperty(HTMLDialogElement.prototype, 'close', close);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'close');
});
describe('Vault membership navigation', () => {
  it('returns to registration when Core removes this existing member', async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const store = AppState();
    const manager = managerFixture();
    mocks.initialize.mockReset().mockResolvedValue(manager);
    await router.push('/');
    wrapper = mount(App, {
      global: {
        plugins: [pinia, router],
        stubs: {
          NavBar: true,
          PasskeyAuth: true,
          Toaster: true,
        },
      },
    });
    await router.isReady();
    await flushPromises();
    expect(store.isMember).toBe(true);
    expect(store.appManager).toBe(manager);
    expect(wrapper.find('.ms-liveboard').exists()).toBe(true);
    expect(wrapper.find('[data-testid="legacy-vault"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="orbit-entry"]').exists()).toBe(false);

    const local = stateFixture().state;
    local.as_info = () => 0;
    local.is_local = () => true;
    local.is_vault = () => false;
    vi.mocked(manager.get_state).mockResolvedValue(local);
    await store.updateState();
    await flushPromises();
    expect(store.currState).toBe(local);
    expect(store.isMember).toBe(false);
    expect(wrapper.find('[data-testid="orbit-entry"]').exists()).toBe(true);
    expect(wrapper.find('.ms-liveboard').exists()).toBe(false);
    expect(wrapper.find('[data-testid="legacy-vault"]').exists()).toBe(false);
  });
});
