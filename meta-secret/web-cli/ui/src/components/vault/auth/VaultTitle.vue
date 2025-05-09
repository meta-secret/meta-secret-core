<script setup>
import { AppState } from '@/stores/app-state';
import { ref } from 'vue';

const jsAppState = AppState();
const vaultName = ref(jsAppState.getVaultName());
</script>

<template>
  <div v-if="vaultName" :class="$style.container">
    <h2 :class="$style.title">
      Creating new vault: <span :class="$style.highlight">{{ vaultName }}</span>
    </h2>
  </div>
</template>

<style module>
.container {
  @apply w-full text-center mb-8 mt-2;
}

.title {
  @apply text-xl text-gray-200 font-medium;
}

.highlight {
  @apply font-bold text-orange-400;
}
</style> 