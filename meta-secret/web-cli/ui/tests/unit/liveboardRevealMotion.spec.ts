import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createRevealMotion } from '@/features/liveboard/revealMotion';

const originalAnimate = Object.getOwnPropertyDescriptor(Element.prototype, 'animate');
beforeEach(() => {
  if (!originalAnimate)
    Object.defineProperty(Element.prototype, 'animate', { configurable: true, writable: true, value: vi.fn() });
});
function setup() {
  const pixels = new Uint8ClampedArray(8 * 8 * 4).fill(255);
  const raster = vi
    .spyOn(HTMLCanvasElement.prototype, 'getContext')
    .mockImplementation(
      () => ({ fillText: vi.fn(), getImageData: () => ({ data: pixels }) }) as unknown as CanvasRenderingContext2D,
    );
  const dimensions = vi.spyOn(document, 'createElement');
  vi.stubGlobal('matchMedia', () => ({ matches: false }));
  const effects: { cancel: ReturnType<typeof vi.fn>; finished: Promise<void>; resolve: () => void }[] = [];
  vi.spyOn(Element.prototype, 'animate').mockImplementation(() => {
    let resolve!: () => void;
    const finished = new Promise<void>((yes) => {
      resolve = yes;
    });
    const effect = { cancel: vi.fn(), finished, resolve };
    effects.push(effect);
    return effect as unknown as Animation;
  });
  const payload = document.createElement('div'),
    value = document.createElement('div');
  const cipher = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  cipher.append(document.createElementNS('http://www.w3.org/2000/svg', 'circle'));
  vi.spyOn(payload, 'getBoundingClientRect').mockReturnValue({ width: 8, height: 8 } as DOMRect);
  const motion = createRevealMotion();
  return { motion, elements: { payload, value, cipher, copy: null }, pixels, raster, dimensions, effects };
}
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  if (!originalAnimate) Reflect.deleteProperty(Element.prototype, 'animate');
});

describe('Liveboard private reveal particle lifetime', () => {
  it('samples the real helper, immediately clears its raster, and cancels derived effects on invalidation', () => {
    const { motion, elements, pixels, raster, dimensions, effects } = setup();
    motion.reveal(elements);
    expect(raster).toHaveBeenCalledOnce();
    expect(motion.activeCount).toBeGreaterThan(0);
    const canvas = dimensions.mock.results.find((result) => result.value instanceof HTMLCanvasElement)
      ?.value as HTMLCanvasElement;
    expect(canvas.width).toBe(0);
    expect(canvas.height).toBe(0);
    expect(pixels.every((value) => value === 0)).toBe(true);
    motion.clear();
    expect(motion.activeCount).toBe(0);
    expect(effects.every((effect) => effect.cancel.mock.calls.length === 1)).toBe(true);
    expect(elements.cipher.getAttribute('viewBox')).toBe('0 0 300 28');
  });
  it('uses existing numeric targets for Hide without sampling another canvas, then clears on completion', async () => {
    const { motion, elements, raster, effects } = setup();
    motion.reveal(elements);
    motion.conceal();
    expect(raster).toHaveBeenCalledOnce();
    expect(motion.activeCount).toBeGreaterThan(0);
    effects.forEach((effect) => effect.resolve());
    await Promise.resolve();
    await Promise.resolve();
    motion.clear();
    expect(motion.activeCount).toBe(0);
  });
  it('clears the raster even when sampling fails, without reporting plaintext or leaving effects', () => {
    const { motion, elements, raster, dimensions } = setup();
    raster.mockImplementation(
      () =>
        ({
          getImageData: () => {
            throw new Error('sensitive failure');
          },
        }) as unknown as CanvasRenderingContext2D,
    );
    expect(() => motion.reveal(elements)).not.toThrow();
    const canvas = dimensions.mock.results.find((result) => result.value instanceof HTMLCanvasElement)
      ?.value as HTMLCanvasElement;
    expect(canvas.width).toBe(0);
    expect(canvas.height).toBe(0);
    expect(motion.activeCount).toBe(0);
  });
  it('reduced motion never samples sensitive text', () => {
    const { motion, elements, raster } = setup();
    vi.stubGlobal('matchMedia', () => ({ matches: true }));
    motion.reveal(elements);
    expect(raster).not.toHaveBeenCalled();
    expect(motion.activeCount).toBe(0);
  });
});

it('releases inverse targets at the generated elapsed deadline even when every effect is stalled', async () => {
  vi.useFakeTimers();
  const { motion, elements, raster } = setup();
  motion.reveal(elements);
  motion.conceal();
  expect(elements.cipher.getAttribute('viewBox')).toBe('0 0 8 8');
  await vi.advanceTimersByTimeAsync(700);
  expect(elements.cipher.getAttribute('viewBox')).toBe('0 0 300 28');
  expect(raster).toHaveBeenCalledOnce();
  motion.clear();
  expect(vi.getTimerCount()).toBe(0);
  vi.useRealTimers();
});

it.each(['pagehide', 'visibilitychange'])(
  'immediately cancels derived motion on %s without waiting for animations',
  (event) => {
    const { motion, elements } = setup();
    motion.reveal(elements);
    motion.conceal();
    expect(motion.activeCount).toBeGreaterThan(0);
    if (event === 'visibilitychange') {
      vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
      document.dispatchEvent(new Event(event));
    } else window.dispatchEvent(new Event(event));
    expect(motion.activeCount).toBe(0);
    expect(elements.cipher.getAttribute('viewBox')).toBe('0 0 300 28');
  },
);

it('cancels an old conceal deadline before a new Show, so it cannot clear the new reveal', async () => {
  vi.useFakeTimers();
  const { motion, elements } = setup();
  motion.reveal(elements);
  motion.conceal();
  await vi.advanceTimersByTimeAsync(300);
  motion.reveal(elements);
  expect(vi.getTimerCount()).toBe(0);
  await vi.advanceTimersByTimeAsync(700);
  expect(motion.activeCount).toBeGreaterThan(0);
  expect(elements.cipher.getAttribute('viewBox')).toBe('0 0 8 8');
  motion.clear();
  vi.useRealTimers();
});
