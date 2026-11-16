import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import InviteQrDialog from '@/components/liveboard/InviteQrDialog.vue';

vi.mock('qr-code-styling', () => ({
  default: class {
    async getRawData() {
      return new Blob(['test SVG']);
    }
    append(host: HTMLElement) {
      host.append(document.createElementNS('http://www.w3.org/2000/svg', 'svg'));
    }
  },
}));

const showModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'showModal');
const close = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'close');
beforeEach(() => {
  vi.stubGlobal('matchMedia', () => ({ matches: true }));
  vi.spyOn(window, 'scrollBy').mockImplementation(() => {});
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
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  if (showModal) Object.defineProperty(HTMLDialogElement.prototype, 'showModal', showModal);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'showModal');
  if (close) Object.defineProperty(HTMLDialogElement.prototype, 'close', close);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'close');
});

describe('Liveboard QR payload ownership', () => {
  it('removes the previous QR immediately when the canonical Vault name becomes unavailable', async () => {
    const anchor = document.createElement('button');
    document.body.append(anchor);
    vi.spyOn(anchor, 'getBoundingClientRect').mockReturnValue({
      top: 100,
      right: 360,
      width: 48,
      height: 48,
    } as DOMRect);
    const wrapper = mount(InviteQrDialog, { props: { open: true, anchor, vaultName: 'Actual Vault' } });
    try {
      await flushPromises();
      expect(wrapper.find('.invite-code svg').exists()).toBe(true);
      await wrapper.setProps({ vaultName: '' });
      await flushPromises();
      expect(wrapper.find('.invite-code svg').exists()).toBe(false);
    } finally {
      wrapper.unmount();
      anchor.remove();
    }
  });
});
