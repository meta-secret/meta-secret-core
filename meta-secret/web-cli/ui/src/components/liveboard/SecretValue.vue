<script setup lang="ts">
import { onBeforeUnmount, ref, watch, type DeepReadonly } from 'vue';
import type { SecretCardView } from '@/features/liveboard/types';
import { liveboardMotion } from '@/design/liveboard/motion';
import { createRevealMotion } from '@/features/liveboard/revealMotion';
const props = defineProps<{ card: DeepReadonly<SecretCardView>; index: number }>();
const payload = ref<HTMLElement | null>(null),
  value = ref<HTMLElement | null>(null),
  cipher = ref<SVGSVGElement | null>(null);
const motion = createRevealMotion();
const grid = liveboardMotion.timelines.cipher;
const dots = Array.from({ length: grid.grid.rows * grid.grid.columns }, (_, index) => {
  const row = Math.floor(index / grid.grid.columns),
    col = index % grid.grid.columns,
    opacity = grid.appearance.opacity;
  return {
    x: grid.origin.x + col * grid.step.x,
    y: grid.origin.y + row * grid.step.y,
    opacity: (
      opacity.base +
      ((row * opacity.rowMultiplier + col * opacity.columnMultiplier + props.index * opacity.indexMultiplier) %
        opacity.modulo) /
        opacity.divisor
    ).toFixed(opacity.roundDigits),
  };
});
watch(
  () => [props.card.value, props.card.conceal] as const,
  ([current, conceal], previous) => {
    // An explicit Hide publishes its conceal intent before clearing value.
    // Keep the existing numeric targets until the actual value transition.
    if (current && current === previous[0]) return;
    if (!current) {
      if (conceal) motion.conceal();
      else motion.clear();
      return;
    }
    motion.clear();
  },
  { flush: 'sync' },
);
// The post-flush observer sees the real rendered glyph nodes. The synchronous
// observer above still clears sensitive effects before any invalidated frame.
watch(
  () => props.card.value,
  (current) => {
    if (!current || !payload.value || !value.value || !cipher.value) return;
    motion.reveal({
      payload: payload.value,
      value: value.value,
      cipher: cipher.value,
      copy: payload.value.parentElement?.querySelector<HTMLElement>('[data-secret-action="copy"]') ?? null,
    });
  },
  { flush: 'post' },
);
onBeforeUnmount(() => {
  motion.clear();
});
</script>
<template>
  <div ref="payload" class="secret-payload" :aria-label="card.value ? 'Revealed value' : 'Secret concealed'">
    <svg ref="cipher" class="secret-cipher" viewBox="0 0 300 28" preserveAspectRatio="none" aria-hidden="true">
      <circle
        v-for="(dot, key) in dots"
        :key="key"
        :cx="dot.x"
        :cy="dot.y"
        :r="grid.appearance.radius"
        :opacity="dot.opacity"
      />
    </svg>
    <div v-if="card.value" ref="value" class="secret-value">
      <p v-if="card.value.kind === 'text'" class="secret-plain">{{ card.value.text }}</p>
      <ol v-else class="secret-seed">
        <li v-for="(word, position) in card.value.words" :key="position">
          <span>{{ word }}</span>
        </li>
      </ol>
    </div>
  </div>
</template>
