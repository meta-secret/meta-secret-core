import { mount, flushPromises } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';
import OrbitQrSheet from '@/components/registration/OrbitQrSheet.vue';

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('QR permission lifecycle', () => {
  it('requests camera only after expansion and releases it on Back', async () => {
    vi.stubGlobal('matchMedia', () => ({ matches: false }));
    let finish!: () => void;
    const expanded = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const animation = { finished: expanded, cancel: vi.fn() };
    Object.defineProperty(HTMLElement.prototype, 'animate', { configurable: true, value: vi.fn(() => animation) });
    const scanner = { start: vi.fn().mockResolvedValue(undefined), stop: vi.fn() };
    const factory = vi.fn(() => scanner);
    const wrapper = mount(OrbitQrSheet, { props: { origin: { x: 100, y: 200, size: 42 }, cameraFactory: factory } });
    await flushPromises();
    expect(factory).not.toHaveBeenCalled();
    finish();
    await flushPromises();
    expect(scanner.start).toHaveBeenCalledOnce();
    await wrapper.get('button[aria-label="Back"]').trigger('click');
    expect(scanner.stop).toHaveBeenCalledOnce();
    await flushPromises();
    expect(wrapper.emitted('close')).toHaveLength(1);
    wrapper.unmount();
  });
  it('closes and releases camera when backgrounded', async () => {
    vi.stubGlobal('matchMedia', () => ({ matches: true }));
    const scanner = { start: vi.fn().mockResolvedValue(undefined), stop: vi.fn() };
    const wrapper = mount(OrbitQrSheet, { props: { origin: { x: 0, y: 0, size: 42 }, cameraFactory: () => scanner } });
    await flushPromises();
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    document.dispatchEvent(new Event('visibilitychange'));
    await flushPromises();
    expect(scanner.stop).toHaveBeenCalledOnce();
    expect(wrapper.emitted('close')).toHaveLength(1);
    wrapper.unmount();
  });
});
