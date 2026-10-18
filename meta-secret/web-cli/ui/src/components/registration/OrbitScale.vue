<script setup lang="ts">
defineProps<{ balanced?: boolean }>();
const ticks = Array.from({ length: 69 }, (_, index) => {
  const k = index - 40;
  const angle = ((180 + k) * Math.PI) / 180;
  const inner = k % 5 === 0 ? 230 : 234;
  return {
    x1: 265 + Math.cos(angle) * inner,
    y1: 130 + Math.sin(angle) * inner,
    x2: 265 + Math.cos(angle) * 241,
    y2: 130 + Math.sin(angle) * 241,
    opacity: Math.max(0, Math.cos(((Math.abs(k) / (k < 0 ? 40 : 28)) * Math.PI) / 2)) ** 1.65,
  };
});
const balancedTicks = Array.from({ length: 79 }, (_, index) => {
  const k = index - 39;
  const angle = ((180 + k) * Math.PI) / 180;
  const major = k % 6 === 0;
  return {
    x1: 520 + Math.cos(angle) * (major ? 496 : 500),
    y1: 210 + Math.sin(angle) * (major ? 496 : 500),
    x2: 520 + Math.cos(angle) * (major ? 512 : 508),
    y2: 210 + Math.sin(angle) * (major ? 512 : 508),
    opacity: Math.max(0, Math.cos(((Math.abs(k) / 39) * Math.PI) / 2)) ** 1.7,
  };
});
</script>
<template>
  <svg class="orbit-ruler" :viewBox="balanced ? '0 0 110 500' : '0 0 90 320'" aria-hidden="true">
    <line v-for="(tick, index) in balanced ? balancedTicks : ticks" :key="index" v-bind="tick" />
  </svg>
</template>
