import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils';
import { reactive } from 'vue';
import { createPinia, setActivePinia } from 'pinia';
import type { WasmApplicationManager } from 'meta-secret-web-cli';
import App from '@/App.vue';
import router from '@/router';
import { registrationSurfaceActive } from '@/features/registration/registrationSurface';
import { managerFixture, stateFixture } from './liveboardCoreFixtures';

const mocks = vi.hoisted(() => ({
  auth: { isAuthenticated: true, masterKey: 'key-a' as string | null },
  manager: null as WasmApplicationManager | null,
  initialize: vi.fn(),
  connect: vi.fn(),
  disconnect: vi.fn(),
  refreshNow: vi.fn(async () => {}),
}));
vi.mock('@/stores/auth', () => ({ useAuthStore: () => mocks.auth }));
vi.mock('@/stores/theme', () => ({ useThemeStore: vi.fn() }));
vi.mock('meta-secret-web-cli', async (importOriginal) => ({
  ...(await importOriginal<typeof import('meta-secret-web-cli')>()),
  default: vi.fn(async () => {}),
  MasterKeyManager: { from_pure_sk: vi.fn(() => ({})) },
  WasmApplicationManager: { init_wasm_with_device_and_environment: mocks.initialize },
}));
vi.mock('@/utils/stateInvalidation', () => ({
  createStateInvalidationController: () => ({
    connect: mocks.connect,
    disconnect: mocks.disconnect,
    refreshNow: mocks.refreshNow,
  }),
}));
vi.mock('@/components/vault/Vault.vue', () => ({
  default: { template: '<div data-testid="legacy-vault"><router-view /></div>' },
}));
vi.mock('@/components/vault/Secrets.vue', () => ({
  default: { template: '<div data-testid="legacy-secrets">Legacy secrets</div>' },
}));
vi.mock('@/components/vault/Devices.vue', () => ({
  default: { template: '<div data-testid="legacy-devices">Legacy devices</div>' },
}));
vi.mock('@/components/registration/OrbitRegistration.vue', () => ({
  default: { template: '<div data-testid="orbit-entry">Registration</div>' },
}));
let wrapper: VueWrapper | undefined;
const showModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'showModal');
const close = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'close');
beforeEach(() => {
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
  vi.clearAllMocks();
  setActivePinia(createPinia());
  registrationSurfaceActive.value = false;
  mocks.auth = reactive({ isAuthenticated: true, masterKey: 'key-a' });
  mocks.manager = managerFixture();
  mocks.initialize.mockImplementation(async () => mocks.manager);
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
async function render(path = '/') {
  await router.push(path);
  wrapper = mount(App, {
    global: {
      plugins: [createPinia(), router],
      stubs: {
        NavBar: { template: '<nav data-testid="global-navbar" />' },
        PasskeyAuth: { template: '<div data-testid="global-passkey" />' },
        Toaster: true,
      },
    },
  });
  await router.isReady();
  await flushPromises();
  return wrapper;
}

describe('Liveboard actual application routing and membership gates', () => {
  it.each(['/', '/secrets', '/profile'])(
    'mounts the real member surface at deep link %s without hidden legacy controls',
    async (path) => {
      const view = await render(path);
      expect(router.currentRoute.value.path).toBe(path);
      expect(view.find('.ms-liveboard').exists()).toBe(true);
      expect(view.find('[data-testid="legacy-vault"]').exists()).toBe(false);
      expect(view.find('[data-testid="legacy-secrets"]').exists()).toBe(false);
      expect(view.find('[data-testid="legacy-devices"]').exists()).toBe(false);
      expect(view.find('[data-testid="global-navbar"]').exists()).toBe(false);
      expect(view.find('[data-testid="global-passkey"]').exists()).toBe(true);
    },
  );

  it('preserves one parent init/SSE through real RouterLink navigation and native Back/Forward', async () => {
    const view = await render('/');
    await view.get('a[href="/secrets"]').trigger('click');
    await flushPromises();
    expect(router.currentRoute.value.path).toBe('/secrets');
    await router.push('/profile');
    const back = new Promise<void>((resolve) => window.addEventListener('popstate', () => resolve(), { once: true }));
    window.history.back();
    await back;
    await flushPromises();
    expect(router.currentRoute.value.path).toBe('/secrets');
    const forward = new Promise<void>((resolve) =>
      window.addEventListener('popstate', () => resolve(), { once: true }),
    );
    window.history.forward();
    await forward;
    await flushPromises();
    expect(router.currentRoute.value.path).toBe('/profile');
    expect(mocks.initialize).toHaveBeenCalledOnce();
    expect(mocks.connect).toHaveBeenCalledExactlyOnceWith('семья@example.test');
  });

  it('keeps semantic href and modifier/new-tab behavior on the actual member links', async () => {
    const view = await render('/');
    const link = view.get('a[href="/secrets"]');
    await link.trigger('click', { ctrlKey: true });
    expect(router.currentRoute.value.path).toBe('/');
    await link.trigger('click', { metaKey: true });
    expect(router.currentRoute.value.path).toBe('/');
    expect(link.attributes('href')).toBe('/secrets');
  });

  it.each(['unauthenticated', 'registration'] as const)('never flashes Liveboard during %s', async (mode) => {
    if (mode === 'unauthenticated') mocks.auth.isAuthenticated = false;
    else {
      const local = stateFixture().state;
      local.as_info = () => 0;
      local.is_local = () => true;
      local.is_vault = () => false;
      vi.mocked(mocks.manager!.get_state).mockResolvedValue(local);
    }
    const view = await render('/secrets');
    expect(view.find('.ms-liveboard').exists()).toBe(false);
    expect(view.find('[data-testid="legacy-vault"]').exists()).toBe(false);
    if (mode === 'registration') expect(view.find('[data-testid="orbit-entry"]').exists()).toBe(true);
  });

  it('redirects /devices to the approved home intent and retains unrelated route records', async () => {
    await render('/devices');
    expect(router.currentRoute.value.path).toBe('/');
    const paths = router.getRoutes().map((route) => route.path);
    for (const path of ['/info', '/settings', '/tools/split', '/tools/recover', '/tools/docs', '/contact'])
      expect(paths).toContain(path);
    await router.push('/info');
    await flushPromises();
    expect(wrapper?.find('[data-testid="global-navbar"]').exists()).toBe(true);
  });
});
