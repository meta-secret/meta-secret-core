import { flushPromises, mount } from '@vue/test-utils';
import { reactive } from 'vue';
import { afterEach, describe, expect, it, vi } from 'vitest';
import VaultView from '@/views/VaultView.vue';
import { registrationSurfaceActive } from '@/features/registration/registrationSurface';

const mocks = vi.hoisted(() => ({
  store: undefined as unknown as {
    isMember: boolean;
    isSynchronizing: boolean;
    syncError: string;
    appManager: null;
    getVaultName: () => string;
    appStateInit: () => Promise<void>;
    updateState: () => Promise<void>;
  },
}));
vi.mock('@/stores/app-state', () => ({ AppState: () => mocks.store }));
vi.mock('@/stores/auth', () => ({ useAuthStore: () => ({ isAuthenticated: true, masterKey: 'test-key' }) }));
vi.mock('vue-router', () => ({ useRoute: () => ({ path: '/' }) }));
vi.mock('@/utils/stateInvalidation', () => ({
  createStateInvalidationController: () => ({ connect: vi.fn(), disconnect: vi.fn(), refreshNow: vi.fn() }),
}));
vi.mock('@/components/vault/Vault.vue', () => ({
  default: { template: '<div data-testid="legacy-vault">Legacy Vault</div>' },
}));
vi.mock('@/components/registration/OrbitRegistration.vue', () => ({
  default: { template: '<div data-testid="orbit-entry">Orbit registration</div>' },
}));

afterEach(() => {
  registrationSurfaceActive.value = false;
});
describe('Vault membership navigation', () => {
  it('returns to registration when Core removes this existing member', async () => {
    mocks.store = reactive({
      isMember: true,
      isSynchronizing: false,
      syncError: '',
      appManager: null,
      getVaultName: () => 'family',
      appStateInit: async () => undefined,
      updateState: async () => undefined,
    });
    const wrapper = mount(VaultView);
    await flushPromises();
    expect(wrapper.find('[data-testid="legacy-vault"]').exists()).toBe(true);
    mocks.store.isMember = false;
    await flushPromises();
    expect(wrapper.find('[data-testid="orbit-entry"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="legacy-vault"]').exists()).toBe(false);
    wrapper.unmount();
  });
});
