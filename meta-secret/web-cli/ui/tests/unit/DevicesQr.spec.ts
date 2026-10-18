import { flushPromises, mount } from '@vue/test-utils';
import { defineComponent, reactive } from 'vue';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import Devices from '@/components/vault/Devices.vue';

const mocks = vi.hoisted(() => ({
  store: undefined as unknown as { name: string; currState: null; getVaultName: () => string },
  create: vi.fn(),
}));
vi.mock('@/stores/app-state', () => ({ AppState: () => mocks.store }));
vi.mock('@/utils/wasmBridge', () => ({ getMemberVaultData: () => undefined, getMemberVaultState: () => undefined }));
vi.mock('meta-secret-web-cli', () => ({ UserDataOutsiderStatus: { Pending: 'Pending', Declined: 'Declined' } }));
vi.mock('@/components/vault/Device.vue', () => ({ default: { template: '<div />' } }));
vi.mock('qr-code-styling', () => ({
  default: class {
    constructor(options: unknown) {
      mocks.create(options);
    }
    async getRawData() {
      return new Blob();
    }
    append(host: HTMLElement) {
      host.appendChild(document.createElementNS('http://www.w3.org/2000/svg', 'svg'));
    }
  },
}));

const dialog = defineComponent({ props: { open: Boolean }, template: '<div v-if="open" role="dialog"><slot /></div>' });
const slots = { template: '<div><slot /></div>' };
function mountDevices() {
  return mount(Devices, {
    global: {
      stubs: {
        Dialog: dialog,
        DialogContent: slots,
        DialogHeader: slots,
        DialogTitle: slots,
        DialogDescription: slots,
        DialogFooter: slots,
      },
    },
  });
}

beforeEach(() => {
  mocks.create.mockClear();
  mocks.store = reactive({
    name: 'Family-Vault',
    currState: null,
    getVaultName() {
      return this.name;
    },
  });
});

describe('Devices Add device QR', () => {
  it('uses the canonical Core name and clears the rendered code on close and unmount', async () => {
    const wrapper = mountDevices();
    expect(mocks.create).not.toHaveBeenCalled();
    await wrapper.get('[data-testid="add-device"]').trigger('click');
    await flushPromises();
    expect(wrapper.get('[data-testid="vault-qr-name"]').text()).toBe('Family-Vault');
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({ data: 'Family-Vault' }));
    const qr = wrapper.get('svg').element;
    await wrapper.get('[data-testid="close-vault-qr"]').trigger('click');
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(qr.parentNode).toBe(null);
    await wrapper.get('[data-testid="add-device"]').trigger('click');
    await flushPromises();
    const host = wrapper.get('[role="img"]').element;
    wrapper.unmount();
    expect(host.childNodes).toHaveLength(0);
  });

  it('disables generation until Core supplies a nonempty name', async () => {
    mocks.store.name = '';
    const wrapper = mountDevices();
    expect(wrapper.get('[data-testid="add-device"]').attributes('disabled')).toBeDefined();
    await wrapper.get('[data-testid="add-device"]').trigger('click');
    expect(mocks.create).not.toHaveBeenCalled();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    mocks.store.name = 'Ready-Vault';
    await flushPromises();
    expect(wrapper.get('[data-testid="add-device"]').attributes('disabled')).toBeUndefined();
    wrapper.unmount();
  });

  it.each(['Another-Vault', ''])('closes a visible QR when canonical name becomes "%s"', async (name) => {
    const wrapper = mountDevices();
    await wrapper.get('[data-testid="add-device"]').trigger('click');
    await flushPromises();
    const qr = wrapper.get('svg').element;
    mocks.store.name = name;
    await flushPromises();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(qr.parentNode).toBe(null);
    wrapper.unmount();
  });
});
