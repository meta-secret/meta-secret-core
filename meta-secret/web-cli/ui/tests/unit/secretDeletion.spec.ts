import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { reactive } from 'vue';
import type { MetaPasswordId } from 'meta-secret-web-cli';
import SecretDeletion from '@/components/vault/SecretDeletion.vue';

const mocks = vi.hoisted(() => ({
  store: null as unknown,
  authenticate: vi.fn(),
  deleteSecret: vi.fn(),
  decide: vi.fn(),
  refresh: vi.fn(),
}));
vi.mock('@/stores/app-state', () => ({ AppState: () => mocks.store }));
vi.mock('@/stores/auth', () => ({ useAuthStore: () => ({ authenticateWithPasskey: mocks.authenticate }) }));
vi.mock('@/utils/wasmBridge', () => ({
  getMemberVaultState: (state: { states: unknown[] }) => ({ deletion_states_json: () => JSON.stringify(state.states) }),
}));
vi.mock('@/components/ui/button', () => ({ Button: { template: '<button><slot /></button>' } }));
vi.mock('@/components/ui/alert-dialog', () => {
  const slot = { template: '<div><slot /></div>' };
  return {
    AlertDialog: { props: ['open'], template: '<div v-if="open"><slot /></div>' },
    AlertDialogContent: slot,
    AlertDialogDescription: slot,
    AlertDialogFooter: slot,
    AlertDialogHeader: slot,
    AlertDialogTitle: slot,
  };
});

const secret = { name: 'Example', id_str: () => 'secret-id' } as unknown as MetaPasswordId;
const policy = { secretId: 'secret-id', requestId: null, requiresApproval: false, clientStatus: null };
beforeEach(() => {
  vi.clearAllMocks();
  mocks.authenticate.mockResolvedValue(true);
  mocks.deleteSecret.mockResolvedValue(undefined);
  mocks.decide.mockResolvedValue(undefined);
  mocks.refresh.mockResolvedValue(undefined);
  mocks.store = reactive({
    currState: { states: [policy] },
    appManager: { delete_secret: mocks.deleteSecret, decide_delete_secret: mocks.decide },
    updateState: mocks.refresh,
  });
});

describe('Secret deletion', () => {
  it('submits only after local confirmation and successful authentication', async () => {
    const wrapper = mount(SecretDeletion, { props: { secret } });
    await wrapper.get('[data-testid="delete-secret-Example"]').trigger('click');
    expect(mocks.deleteSecret).not.toHaveBeenCalled();
    await wrapper.get('[data-testid="confirm-delete-secret"]').trigger('click');
    await flushPromises();
    expect(mocks.authenticate).toHaveBeenCalledOnce();
    expect(mocks.deleteSecret).toHaveBeenCalledOnce();
    expect(mocks.decide).not.toHaveBeenCalled();
    expect(mocks.refresh).toHaveBeenCalledOnce();
  });

  it('uses the exact Core request ID for remote approval', async () => {
    mocks.store = reactive({
      currState: {
        states: [policy, { ...policy, requestId: 'request-uuid', requiresApproval: true, clientStatus: 'NeedApprove' }],
      },
      appManager: { delete_secret: mocks.deleteSecret, decide_delete_secret: mocks.decide },
      updateState: mocks.refresh,
    });
    const wrapper = mount(SecretDeletion, { props: { secret } });
    await wrapper.get('[data-testid="approve-secret-deletion"]').trigger('click');
    await flushPromises();
    expect(mocks.decide).toHaveBeenCalledWith('request-uuid', true);
    expect(mocks.deleteSecret).not.toHaveBeenCalled();
  });

  it('keeps the Secret and reports an error when authentication is cancelled', async () => {
    mocks.authenticate.mockResolvedValue(false);
    const wrapper = mount(SecretDeletion, { props: { secret } });
    await wrapper.get('[data-testid="delete-secret-Example"]').trigger('click');
    await wrapper.get('[data-testid="confirm-delete-secret"]').trigger('click');
    await flushPromises();
    expect(mocks.deleteSecret).not.toHaveBeenCalled();
    expect(wrapper.get('[role="alert"]').text()).toContain('cancelled');
    expect(wrapper.get('[data-testid="confirm-delete-secret"]').attributes('disabled')).toBeUndefined();
  });
});
