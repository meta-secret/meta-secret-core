import { afterEach, describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { defineComponent, h, ref } from 'vue';
import { useMeasuredHeight } from '@/features/liveboard/motion';

const originalAnimate = Object.getOwnPropertyDescriptor(Element.prototype, 'animate');
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  if (originalAnimate) Object.defineProperty(Element.prototype, 'animate', originalAnimate);
  else Reflect.deleteProperty(Element.prototype, 'animate');
});

describe('Liveboard measured height does not feed its own ResizeObserver', () => {
  it('disconnects before writing animated height and only reobserves after completion', async () => {
    let callback!: ResizeObserverCallback;
    const observe = vi.fn(),
      disconnect = vi.fn();
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor(received: ResizeObserverCallback) {
          callback = received;
        }
        observe = observe;
        disconnect = disconnect;
      },
    );
    vi.stubGlobal('matchMedia', () => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() }));
    const frames: FrameRequestCallback[] = [];
    vi.stubGlobal('requestAnimationFrame', (frame: FrameRequestCallback) => {
      frames.push(frame);
      return frames.length;
    });
    vi.stubGlobal('cancelAnimationFrame', vi.fn());
    let height = 100,
      finish!: () => void;
    const finished = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const cancel = vi.fn();
    Object.defineProperty(Element.prototype, 'animate', {
      configurable: true,
      value: vi.fn(() => {
        expect(disconnect).toHaveBeenCalledOnce();
        return { finished, cancel, playState: 'running' };
      }),
    });
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(() => ({ height }) as DOMRect);
    const fixture = defineComponent({
      setup() {
        const element = ref<HTMLElement | null>(null);
        useMeasuredHeight(element, 'secretHeight');
        return () => h('div', { ref: element });
      },
    });
    const wrapper = mount(fixture);
    try {
      expect(observe).toHaveBeenCalledOnce();
      height = 200;
      callback([], {} as ResizeObserver);
      // ResizeObserver delivery itself performs no layout write.
      expect(Element.prototype.animate).not.toHaveBeenCalled();
      frames.splice(0).forEach((frame) => frame(0));
      expect(Element.prototype.animate).toHaveBeenCalledOnce();
      callback([], {} as ResizeObserver);
      expect(frames).toHaveLength(0);
      finish();
      await Promise.resolve();
      await Promise.resolve();
      expect(observe).toHaveBeenCalledTimes(2);
    } finally {
      wrapper.unmount();
    }
    expect(cancel).toHaveBeenCalled();
  });
});
