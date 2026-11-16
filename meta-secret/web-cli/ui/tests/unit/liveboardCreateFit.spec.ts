import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import CreateSecretDialog from '@/components/liveboard/CreateSecretDialog.vue';
import { liveboardCreateFit } from '@/design/liveboard/runtime';
import type { CreateView } from '@/features/liveboard/types';

const showModal = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'showModal');
const close = Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, 'close');
const viewport = new EventTarget() as EventTarget & { height: number };
beforeEach(() => {
  vi.stubGlobal('innerWidth', 1280);
  vi.stubGlobal('innerHeight', 900);
  viewport.height = 900;
  vi.stubGlobal('visualViewport', viewport);
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
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  if (showModal) Object.defineProperty(HTMLDialogElement.prototype, 'showModal', showModal);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'showModal');
  if (close) Object.defineProperty(HTMLDialogElement.prototype, 'close', close);
  else Reflect.deleteProperty(HTMLDialogElement.prototype, 'close');
});
const view = (): CreateView => ({
  open: true,
  phase: 'idle',
  receipt: null,
  pasteHint: '',
  draft: { mode: 'seed', name: '', value: '', words: Array<string>(24).fill(''), wordCount: 24 },
});

function setup(frame = { left: 0, width: 1280 }) {
  const host = document.createElement('main');
  host.className = 'ms-liveboard';
  document.body.append(host);
  vi.spyOn(host, 'getBoundingClientRect').mockReturnValue(frame as DOMRect);
  const wrapper = mount(CreateSecretDialog, { props: { view: view(), blocked: false }, attachTo: host });
  const dialog = wrapper.get('dialog').element as HTMLDialogElement;
  return { wrapper, dialog, host };
}

describe('Liveboard Create consumes canonical scripted fit', () => {
  it('applies reviewed geometry at open and keyboard visual-viewport resize, with no top/bottom override', async () => {
    const { wrapper, dialog, host } = setup();
    try {
      await flushPromises();
      const expected = liveboardCreateFit({ left: 0, width: 1280 }, { width: 1280, height: 900, visualHeight: 900 });
      expect(dialog.style.getPropertyValue('--create-max')).toBe(`${expected.maxHeight}px`);
      expect(dialog.style.width).toBe(`${expected.width}px`);
      expect(dialog.style.top).toBe('');
      expect(dialog.style.bottom).toBe('');
      viewport.height = 520;
      viewport.dispatchEvent(new Event('resize'));
      expect(dialog.style.getPropertyValue('--create-max')).toBe(
        `${liveboardCreateFit({ left: 0, width: 1280 }, { width: 1280, height: 900, visualHeight: 520 }).maxHeight}px`,
      );
      const rendered = dialog.style.cssText;
      wrapper.unmount();
      viewport.height = 300;
      viewport.dispatchEvent(new Event('resize'));
      expect(dialog.style.cssText).toBe(rendered);
    } finally {
      if (wrapper.exists()) wrapper.unmount();
      host.remove();
    }
  });
  it('preserves embedded frame positioning and recomputes on window resize', async () => {
    const frame = { left: 100, width: 390 };
    const { wrapper, dialog, host } = setup(frame);
    try {
      await flushPromises();
      const expected = liveboardCreateFit(frame, { width: 1280, height: 900, visualHeight: 900 });
      expect(dialog.style.left).toBe(`${expected.left}px`);
      expect(dialog.style.right).toBe('auto');
      vi.stubGlobal('innerWidth', 390);
      window.dispatchEvent(new Event('resize'));
      expect(dialog.style.left).toBe('0px');
      expect(dialog.style.right).toBe('0px');
      expect(dialog.style.width).toBe(
        `${liveboardCreateFit(frame, { width: 390, height: 900, visualHeight: 900 }).width}px`,
      );
    } finally {
      wrapper.unmount();
      host.remove();
    }
  });
  it('recomputes on reopening while keeping a closed form untouched by viewport events', async () => {
    const { wrapper, dialog, host } = setup();
    try {
      await flushPromises();
      const closed = { ...view(), open: false };
      await wrapper.setProps({ view: closed });
      const old = dialog.style.cssText;
      viewport.height = 450;
      viewport.dispatchEvent(new Event('resize'));
      expect(dialog.style.cssText).toBe(old);
      await wrapper.setProps({ view: { ...closed, open: true } });
      expect(dialog.style.getPropertyValue('--create-max')).toBe(
        `${liveboardCreateFit({ left: 0, width: 1280 }, { width: 1280, height: 900, visualHeight: 450 }).maxHeight}px`,
      );
    } finally {
      wrapper.unmount();
      host.remove();
    }
  });
});
