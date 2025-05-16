<script setup>
import { computed } from 'vue';

const props = defineProps({
  progress: {
    type: Number,
    default: 0
  },
  color: {
    type: String,
    default: 'green'
  },
  height: {
    type: String,
    default: '3px'
  }
});

const progressStyle = computed(() => ({
  width: `${Math.min(props.progress, 100)}%`,
  height: props.height,
  backgroundColor: props.color === 'green' ? '#22c55e' : 
                   props.color === 'yellow' ? '#eab308' : props.color
}));
</script>

<template>
  <div :class="$style.progressBarContainer">
    <div :class="$style.progressBar" :style="progressStyle"></div>
  </div>
</template>

<style module>
.progressBarContainer {
  @apply w-full bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden;
}

.progressBar {
  @apply rounded-full transition-all duration-200;
}
</style>
