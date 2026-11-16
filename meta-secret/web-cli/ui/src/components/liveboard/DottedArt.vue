<script setup lang="ts">
import { computed } from 'vue';
import { liveboardGeometry } from '@/design/liveboard/geometry';
export type ArtName = keyof typeof liveboardGeometry.art;
const props = defineProps<{ name: ArtName }>();
const art = computed(() => liveboardGeometry.art[props.name]);
</script>
<template>
  <svg :viewBox="art.viewBox.join(' ')" aria-hidden="true">
    <circle v-for="(dot, index) in art.dots" :key="index" :cx="dot.x" :cy="dot.y" :r="dot.radius" :fill="dot.color" />
  </svg>
</template>
