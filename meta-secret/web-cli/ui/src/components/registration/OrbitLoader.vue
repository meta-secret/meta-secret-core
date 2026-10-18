<script setup lang="ts">
defineProps<{ central?: boolean; origin?: 'left' | 'center' }>();
</script>
<template>
  <div class="orbit-loader" :class="[{ 'has-center': central }, origin && `from-${origin}`]" aria-hidden="true">
    <i v-for="n in 13" :key="n" :style="{ '--bar': n }" />
  </div>
</template>
