import { liveboardMotion } from '@/design/liveboard/motion';

interface Point {
  x: number;
  y: number;
}
interface RevealElements {
  payload: HTMLElement;
  value: HTMLElement;
  cipher: SVGSVGElement;
  copy: HTMLElement | null;
}

/** Private ephemeral raster, never serialized or retained after point sampling. */
function sampleValue(elements: RevealElements) {
  const bounds = elements.payload.getBoundingClientRect();
  const canvas = document.createElement('canvas');
  let pixels: Uint8ClampedArray | undefined;
  const points: Point[] = [];
  try {
    canvas.width = Math.max(1, Math.ceil(bounds.width));
    canvas.height = Math.max(1, Math.ceil(bounds.height));
    const context = canvas.getContext('2d');
    if (!context) return { width: bounds.width, height: bounds.height, points };
    context.fillStyle = '#fff';
    context.textBaseline = 'top';
    const walker = document.createTreeWalker(elements.value, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const node = walker.currentNode as Text;
      const style = getComputedStyle(node.parentElement!);
      context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
      for (let index = 0; index < node.length; index++) {
        if (!node.textContent?.[index]?.trim()) continue;
        const range = document.createRange();
        try {
          range.setStart(node, index);
          range.setEnd(node, index + 1);
          const letter = range.getBoundingClientRect();
          context.fillText(node.textContent[index]!, letter.left - bounds.left, letter.top - bounds.top);
        } finally {
          range.detach();
        }
      }
    }
    pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
    const sampling = liveboardMotion.timelines.particleSampling;
    for (let y = sampling.originPx; y < canvas.height; y += sampling.stepPx)
      for (let x = sampling.originPx; x < canvas.width; x += sampling.stepPx)
        if ((pixels[(y * canvas.width + x) * 4 + 3] ?? 0) > sampling.alphaThreshold) points.push({ x, y });
    return { width: bounds.width, height: bounds.height, points };
  } finally {
    pixels?.fill(0);
    pixels = undefined;
    canvas.width = 0;
    canvas.height = 0;
    canvas.remove();
  }
}

interface Particle {
  x: number;
  y: number;
  dx: number;
  dy: number;
  opacity: number;
}

/** Only numeric targets survive the authorized reveal; never a retained raster or string. */
export function createRevealMotion() {
  const effects = new Set<Animation>();
  let revision = 0;
  const particles: Particle[] = [];
  let cipher: SVGSVGElement | null = null;
  let concealDeadline: ReturnType<typeof setTimeout> | null = null;
  const visibility = () => {
    if (document.visibilityState === 'hidden') clear();
  };
  function unbindPrivacy() {
    window.removeEventListener('pagehide', clear);
    document.removeEventListener('visibilitychange', visibility);
  }
  function bindPrivacy() {
    window.addEventListener('pagehide', clear);
    document.addEventListener('visibilitychange', visibility);
  }
  function stop() {
    revision++;
    if (concealDeadline !== null) {
      clearTimeout(concealDeadline);
      concealDeadline = null;
    }
    for (const effect of effects) effect.cancel();
    effects.clear();
  }
  function resetCipher() {
    if (!cipher) return;
    cipher.style.height = '';
    cipher.style.overflow = '';
    cipher.setAttribute('viewBox', '0 0 300 28');
    const grid = liveboardMotion.timelines.cipher;
    [...cipher.querySelectorAll('circle')].forEach((circle, index) => {
      circle.setAttribute('cx', String(grid.origin.x + (index % grid.grid.columns) * grid.step.x));
      circle.setAttribute('cy', String(grid.origin.y + Math.floor(index / grid.grid.columns) * grid.step.y));
    });
  }
  function clear() {
    stop();
    unbindPrivacy();
    particles.length = 0;
    resetCipher();
    cipher = null;
  }
  function add(options: {
    element: Element | null;
    frames: Keyframe[];
    name: 'cipherFade' | 'cipherConcealFade' | 'cipherParticle' | 'cipherParticleHide' | 'valueReveal' | 'copyReveal';
    index?: number;
  }) {
    if (!options.element?.animate) return null;
    const timing = liveboardMotion.timelines.transitions[options.name].timing;
    const delay =
      timing.delay.kind === 'indexed'
        ? ((options.index ?? 0) % timing.delay.modulo) * timing.delay.stepMs
        : timing.delay.ms;
    const effect = options.element.animate(options.frames, {
      duration: timing.duration.ms,
      delay,
      easing: timing.easing,
      fill: timing.fill,
    });
    effects.add(effect);
    return effect;
  }
  function reveal(elements: RevealElements) {
    clear();
    const token = revision;
    if (!elements.payload.animate || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    let sampled: ReturnType<typeof sampleValue> | null = null;
    try {
      bindPrivacy();
      sampled = sampleValue(elements);
      cipher = elements.cipher;
      const grid = liveboardMotion.timelines.cipher;
      cipher.setAttribute('viewBox', `0 0 ${sampled.width} ${Math.max(1, sampled.height)}`);
      cipher.style.height = `${Math.max(1, sampled.height)}px`;
      cipher.style.overflow = 'visible';
      add({
        element: cipher,
        frames: [{ opacity: 1 }, { opacity: 0.9, offset: 0.58 }, { opacity: 0 }],
        name: 'cipherFade',
      });
      const circles = [...cipher.querySelectorAll('circle')];
      circles.forEach((circle, index) => {
        const x = ((grid.origin.x + (index % grid.grid.columns) * grid.step.x) * sampled!.width) / grid.viewport.width;
        const y = grid.origin.y + Math.floor(index / grid.grid.columns) * grid.step.y;
        const target = sampled!.points[Math.floor((index * sampled!.points.length) / circles.length)] ?? { x, y };
        const particle = {
          x,
          y,
          dx: target.x - x,
          dy: target.y - y,
          opacity: Number(circle.getAttribute('opacity') ?? 1),
        };
        particles.push(particle);
        circle.setAttribute('cx', String(x));
        circle.setAttribute('cy', String(y));
        add({
          element: circle,
          frames: [
            { transform: 'translate(0,0)', opacity: particle.opacity },
            { transform: `translate(${particle.dx}px,${particle.dy}px)`, opacity: 0.85 },
          ],
          name: 'cipherParticle',
          index,
        });
      });
      add({
        element: elements.value,
        frames: [
          { opacity: 0, filter: 'blur(2px)' },
          { opacity: 0, filter: 'blur(2px)', offset: 0.48 },
          { opacity: 1, filter: 'blur(0)' },
        ],
        name: 'valueReveal',
      });
      add({
        element: elements.copy,
        frames: [
          { opacity: 0, transform: 'translateX(-8px) scale(.8)' },
          { opacity: 1, transform: 'none' },
        ],
        name: 'copyReveal',
      });
      void Promise.allSettled([...effects].map((effect) => effect.finished)).then(() => {
        if (token !== revision) return;
        // Keep only the135 assigned numeric targets while that same value remains authorized.
        effects.forEach((effect) => effect.cancel());
        effects.clear();
      });
    } catch {
      clear();
    } finally {
      if (sampled) sampled.points.length = 0;
      sampled = null;
    }
  }
  function conceal() {
    stop();
    const token = revision;
    if (!cipher || !particles.length || matchMedia('(prefers-reduced-motion: reduce)').matches) {
      clear();
      return;
    }
    const inverses: Animation[] = [];
    try {
      add({ element: cipher, frames: [{ opacity: 0.7 }, { opacity: 1 }], name: 'cipherConcealFade' });
      [...cipher.querySelectorAll('circle')].forEach((circle, index) => {
        const particle = particles[index];
        if (!particle) return;
        const effect = add({
          element: circle,
          frames: [
            { transform: `translate(${particle.dx}px,${particle.dy}px)`, opacity: 0.85 },
            { transform: 'translate(0,0)', opacity: particle.opacity },
          ],
          name: 'cipherParticleHide',
          index,
        });
        if (effect) inverses.push(effect);
      });
      // Exactly640ms plus indexed delays up to60ms, never a domain completion timer.
      const releaseTargets = () => {
        if (token !== revision) return;
        if (concealDeadline !== null) {
          clearTimeout(concealDeadline);
          concealDeadline = null;
        }
        inverses.forEach((effect) => {
          effect.cancel();
          effects.delete(effect);
        });
        particles.length = 0;
        resetCipher();
        unbindPrivacy();
      };
      const timing = liveboardMotion.timelines.transitions.cipherParticleHide.timing;
      const maxDelay = (timing.delay.modulo - 1) * timing.delay.stepMs;
      // Privacy deadline survives a paused/stalled WAAPI effect; it never changes domain state.
      concealDeadline = setTimeout(releaseTargets, timing.duration.ms + maxDelay);
      void Promise.allSettled(inverses.map((effect) => effect.finished)).then(releaseTargets);
    } catch {
      clear();
    }
  }
  return {
    reveal,
    conceal,
    clear,
    get activeCount() {
      return effects.size;
    },
  };
}
