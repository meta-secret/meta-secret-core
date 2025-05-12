<script setup lang="ts">
import { ref, onMounted } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';

import { AppState } from '@/stores/app-state';

const jsAppState = AppState();
const isInitialized = ref(false);

onMounted(async () => {
  await jsAppState.appStateInit();
  isInitialized.value = true;
});
</script>

<template>
  <div class="py-3"></div>
  <div :class="$style.headerContainer">
    <div :class="$style.alphaBadge">Alpha Version</div>
  </div>

  <div class="flex justify-center py-6">
    <p class="text-2xl font-bold text-gray-900 dark:text-gray-100">Decentralized Secret Manager</p>
  </div>

  <div v-if="!isInitialized" class="text-center mt-8">
    <p class="text-gray-400">Loading Vault Information...</p>
  </div>

  <div v-else-if="!jsAppState.isMember">
    <RegistrationComponent />
  </div>
  <div v-else-if="jsAppState.isMember">
    <VaultComponent />
  </div>
</template>

<style module>
.headerContainer {
  @apply container mx-auto flex flex-col items-center max-w-md;
  position: relative;
}

.alphaBadge {
  @apply absolute -top-1 right-2 bg-red-500 text-white text-xs font-bold px-2 py-0.5 rounded;
  @apply shadow-md shadow-red-900/100;
  @apply uppercase tracking-wide;
  font-size: 0.65rem;
  transform: rotate(5deg);
}
</style>