<script setup lang="ts">
import { computed } from 'vue';
import { liveboardGeometry } from '@/design/liveboard/geometry';
import SvgPrimitive, { type Primitive } from './SvgPrimitive.vue';

export type IconName = keyof typeof liveboardGeometry.icons;
const props = defineProps<{ name: IconName }>();
interface Part {
  part: string;
  target: string;
  primitives: Primitive[];
}
const icon = computed(
  () =>
    liveboardGeometry.icons[props.name] as unknown as {
      reference?: string;
      parts: Part[];
      viewBox?: number[];
      strokeWidth?: number;
      fill?: boolean;
    },
);
</script>
<template>
  <svg
    :viewBox="icon.viewBox?.join(' ') ?? '0 0 24 24'"
    :fill="icon.fill ? 'currentColor' : 'none'"
    :stroke="icon.fill ? 'none' : 'currentColor'"
    :stroke-width="icon.strokeWidth ?? 1.35"
    :class="icon.parts.filter((part) => part.target === 'svg').map((part) => part.part)"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    :data-icon="icon.reference"
  >
    <template v-for="(part, index) in icon.parts" :key="index">
      <g
        v-if="part.target === 'group'"
        :class="part.part"
        :fill="icon.fill ? 'currentColor' : undefined"
        :stroke="icon.fill ? 'none' : undefined"
      >
        <SvgPrimitive v-for="(primitive, item) in part.primitives" :key="item" :primitive="primitive" />
      </g>
      <template v-else>
        <SvgPrimitive
          v-for="(primitive, item) in part.primitives"
          :key="item"
          :primitive="primitive"
          :part="part.target === 'svg' ? undefined : part.part"
        />
      </template>
    </template>
  </svg>
</template>
