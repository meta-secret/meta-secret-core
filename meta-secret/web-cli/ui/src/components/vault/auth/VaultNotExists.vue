<script setup>
import { AppState } from '@/stores/app-state';
import { ref } from 'vue';

const props = defineProps({
  signUpProcessing: Boolean
});

const emit = defineEmits(['create']);

const jsAppState = AppState();
const vaultName = ref(jsAppState.getVaultName());

const createVault = () => {
  emit('create');
};
</script>

<template>
  <div v-if="jsAppState.isVaultNotExists" :class="$style.container">
    <div :class="$style.statusContainer">
      <label :class="$style.statusLabel">Vault name is free!</label>
      <button :class="$style.actionButton" @click="createVault" :disabled="signUpProcessing">Create</button>
    </div>
  </div>
</template>

<style module>
.container {
  @apply w-full mb-8;
}

.statusContainer {
  @apply flex items-center justify-between;
  @apply py-5 px-6 rounded-lg;
  @apply bg-gray-800/80 border border-gray-700;
  @apply shadow-lg transition-all duration-200;
  @apply w-full;
}

.statusLabel {
  @apply text-gray-200 text-lg font-medium;
  @apply mr-4;
}

.actionButton {
  @apply bg-orange-600 hover:bg-orange-700 text-white;
  @apply font-medium py-3 px-8 rounded-lg;
  @apply transition-colors duration-200 shadow-md;
  @apply text-base whitespace-nowrap;
  @apply ml-auto;
}

.actionButton:disabled {
  @apply bg-gray-500 cursor-not-allowed;
}
</style>
