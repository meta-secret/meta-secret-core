<script setup>
import { AppState } from '@/stores/app-state';

defineProps({
  signUpProcessing: Boolean
});

const emit = defineEmits(['create']);

const jsAppState = AppState();

const createVault = () => {
  emit('create');
};
</script>

<template>
  <div v-if="jsAppState.isVaultNotExists" :class="$style.container">
    <div :class="$style.statusContainer">
      <div :class="$style.statusContent">
        <label :class="$style.statusLabel">Vault name is free!</label>
        <button :class="$style.actionButton" @click="createVault" :disabled="signUpProcessing">Create</button>
      </div>
    </div>
  </div>
</template>

<style module>
.container {
  @apply w-full;
}

.statusContainer {
  @apply py-4 px-5 rounded-lg;
  @apply bg-gray-800 border border-gray-700;
  @apply shadow-lg transition-all duration-200;
}

.statusContent {
  @apply flex items-center justify-between;
  @apply gap-4; /* Small gap between text and button for readability */
}

.statusLabel {
  @apply text-gray-300 text-base font-medium;
  @apply flex-grow; /* Take up available space */
}

.actionButton {
  @apply bg-orange-600 hover:bg-orange-700 text-white font-medium py-2 px-5 rounded-lg;
  @apply transition-colors duration-200 shadow-md;
  @apply text-base whitespace-nowrap;
  @apply flex-shrink-0; /* Prevent button from shrinking */
  @apply min-w-[80px]; /* Ensure minimum width */
}

.actionButton:disabled {
  @apply bg-gray-500 cursor-not-allowed;
}
</style>
