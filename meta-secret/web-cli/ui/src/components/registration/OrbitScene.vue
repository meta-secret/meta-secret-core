<script setup lang="ts">
import { orbitTokens } from '@/design/orbit/tokens';
defineProps<{ step: number; direction: number; balanced?: boolean }>();
const emit = defineEmits<{ settled: [] }>();
function animateScene(element: Element, done: () => void, enter: boolean, direction: number, balanced = false) {
  const node = element as HTMLElement;
  if (!node.animate || window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    done();
    if (enter) emit('settled');
    return;
  }
  node.inert = true;
  if (!enter) node.setAttribute('aria-hidden', 'true');
  node.style.transformOrigin = balanced
    ? '895px 210px'
    : `${node.clientWidth - orbitTokens.orbitDimensionPivotRightInset}px ${node.clientHeight / 2}px`;
  const frames = balanced
    ? enter
      ? [
          { transform: `rotate(${-22 * direction}deg)`, opacity: 0 },
          { transform: 'rotate(0deg)', opacity: 1 },
        ]
      : [
          { transform: 'rotate(0deg)', opacity: 1 },
          { transform: `rotate(${22 * direction}deg)`, opacity: 0 },
        ]
    : enter
      ? [
          { transform: `rotate(${-40 * direction}deg)`, opacity: 0 },
          { transform: `rotate(${-24 * direction}deg)`, opacity: 0.1, offset: 0.4 },
          { transform: `rotate(${-12 * direction}deg)`, opacity: 0.6, offset: 0.7 },
          { transform: 'rotate(0deg)', opacity: 1 },
        ]
      : [
          { transform: 'rotate(0deg)', opacity: 1 },
          { transform: `rotate(${20 * direction}deg)`, opacity: 0.25, offset: 0.5 },
          { transform: `rotate(${32 * direction}deg)`, opacity: 0, offset: 0.8 },
          { transform: `rotate(${40 * direction}deg)`, opacity: 0 },
        ];
  const animation = node.animate(frames, {
    duration: balanced ? 760 : orbitTokens.orbitDurationScene,
    easing: 'cubic-bezier(.4,0,.2,1)',
  });
  animation.finished
    .catch(() => undefined)
    .then(() => {
      node.inert = false;
      done();
      if (enter) emit('settled');
    });
}
</script>
<template>
  <Transition
    :css="false"
    @enter="(el, done) => animateScene(el, done, true, direction, balanced)"
    @leave="(el, done) => animateScene(el, done, false, direction, balanced)"
  >
    <div :key="step" class="orbit-turn">
      <svg v-if="balanced" class="orbit-step" viewBox="0 0 900 500" role="img" :aria-label="`Step ${step}`">
        <circle cx="82" cy="210" r="23" />
        <text x="82" y="210" text-anchor="middle" dominant-baseline="central">{{ String(step).padStart(2, '0') }}</text>
      </svg>
      <svg v-else class="orbit-step" viewBox="0 0 90 320" role="img" :aria-label="`Step ${step}`">
        <circle cx="24" cy="130" r="13" />
        <text x="24" y="130" text-anchor="middle" dominant-baseline="central">{{ String(step).padStart(2, '0') }}</text>
      </svg>
      <slot />
    </div>
  </Transition>
</template>
