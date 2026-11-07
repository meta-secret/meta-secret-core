<script setup lang="ts">
import { computed } from 'vue';
import { liveboardTextPoints } from '@/design/liveboard/runtime';
import { liveboardGeometry } from '@/design/liveboard/geometry';
const props = defineProps<{ text: string }>();
const supported = computed(() =>
  [...props.text.toUpperCase()].every((char) => char in liveboardGeometry.glyphs.glyphs),
);
const geometry = computed(() => (supported.value ? liveboardTextPoints(props.text) : null));
</script>
<template>
  <span :data-dot="text">
    <svg
      v-if="geometry"
      class="dot-text"
      :viewBox="`0 0 ${geometry.width} ${geometry.height}`"
      role="img"
      :aria-label="text"
    >
      <circle
        v-for="(point, index) in geometry.points"
        :key="index"
        :cx="point.x"
        :cy="point.y"
        :r="point.radius"
        fill="currentColor"
      />
    </svg>
    <template v-else>{{ text }}</template>
  </span>
</template>
