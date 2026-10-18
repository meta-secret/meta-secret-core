import { flushPromises, mount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import VaultNameQr from '@/components/vault/VaultNameQr.vue';

const encoder = vi.hoisted(() => ({
  create: vi.fn(),
  ready: vi.fn<() => Promise<unknown>>(),
  append: vi.fn<(host: HTMLElement) => void>(),
}));

vi.mock('qr-code-styling', () => ({
  default: class {
    constructor(options: unknown) {
      encoder.create(options);
    }
    getRawData() {
      return encoder.ready();
    }
    append(host: HTMLElement) {
      encoder.append(host);
    }
  },
}));

beforeEach(() => {
  encoder.create.mockReset();
  encoder.ready.mockReset().mockResolvedValue(new Blob());
  encoder.append.mockReset().mockImplementation((host) => {
    host.appendChild(document.createElementNS('http://www.w3.org/2000/svg', 'svg'));
  });
});
afterEach(() => vi.restoreAllMocks());

describe('Vault name QR presentation', () => {
  it.each(['Dmitrii@Example.org', 'random@privaterelay.appleid.com', 'Family-CLI-Vault'])(
    'encodes the exact canonical name %s with a readable quiet zone',
    async (vaultName) => {
      const wrapper = mount(VaultNameQr, { props: { vaultName } });
      await flushPromises();
      expect(encoder.create).toHaveBeenCalledWith(
        expect.objectContaining({
          data: vaultName,
          type: 'svg',
          width: 256,
          height: 256,
          margin: 40,
          dotsOptions: { type: 'square', color: '#000000' },
          backgroundOptions: { color: '#ffffff' },
        }),
      );
      expect(wrapper.find('[role="img"]').attributes('aria-label')).toBe('Vault name QR code');
      expect(wrapper.find('svg').exists()).toBe(true);
      wrapper.unmount();
    },
  );

  it('never encodes an empty name', async () => {
    const wrapper = mount(VaultNameQr, { props: { vaultName: '' } });
    await flushPromises();
    expect(encoder.create).not.toHaveBeenCalled();
    expect(wrapper.find('svg').exists()).toBe(false);
    wrapper.unmount();
  });

  it('clears an old code before drawing the changed canonical name', async () => {
    const wrapper = mount(VaultNameQr, { props: { vaultName: 'Old-Vault' } });
    await flushPromises();
    const oldSvg = wrapper.find('svg').element;
    await wrapper.setProps({ vaultName: 'Next-Vault' });
    expect(wrapper.element.contains(oldSvg)).toBe(false);
    await flushPromises();
    expect(encoder.create).toHaveBeenLastCalledWith(expect.objectContaining({ data: 'Next-Vault' }));
    expect(wrapper.findAll('svg')).toHaveLength(1);
    await wrapper.setProps({ vaultName: '' });
    expect(wrapper.find('svg').exists()).toBe(false);
    wrapper.unmount();
  });

  it('does not append a stale asynchronous result after name change or unmount', async () => {
    let finish!: (value: unknown) => void;
    encoder.ready.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const wrapper = mount(VaultNameQr, { props: { vaultName: 'Old-Vault' } });
    await wrapper.setProps({ vaultName: '' });
    finish(new Blob());
    await flushPromises();
    expect(encoder.append).not.toHaveBeenCalled();

    encoder.ready.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    await wrapper.setProps({ vaultName: 'Next-Vault' });
    const host = wrapper.find('[role="img"]').element;
    wrapper.unmount();
    finish(new Blob());
    await flushPromises();
    expect(encoder.append).not.toHaveBeenCalled();
    expect(host.childNodes).toHaveLength(0);
  });

  it.each(['create', 'ready'] as const)('handles encoder %s failures without a QR or raw error', async (stage) => {
    const rawError = new Error('encoder details must stay private');
    if (stage === 'create')
      encoder.create.mockImplementationOnce(() => {
        throw rawError;
      });
    else encoder.ready.mockRejectedValueOnce(rawError);
    const wrapper = mount(VaultNameQr, { props: { vaultName: 'Family-Vault' } });
    await flushPromises();
    expect(wrapper.get('[role="alert"]').text()).toBe('Could not create QR code. Close and try again.');
    expect(wrapper.find('svg').exists()).toBe(false);
    expect(wrapper.text()).not.toContain(rawError.message);
    wrapper.unmount();
  });
});
