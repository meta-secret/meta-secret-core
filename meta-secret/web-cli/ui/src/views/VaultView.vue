<script setup lang="ts">
import { ref, onMounted } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';

import { AppState } from '@/stores/app-state';

const jsAppState = AppState();
const isInitialized = ref(false);
const isCleaning = ref(false);

async function cleanDatabase() {
  if (isCleaning.value) return;
  if (!confirm('Are you sure you want to delete all vault data and start over? This action cannot be undone.')) return;
  isCleaning.value = true;
  try {
    await (jsAppState.appManager as any).clean_up_database();
    await jsAppState.appStateInit();
  } finally {
    isCleaning.value = false;
  }
}

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

  <div class="flex justify-center mb-4">
    <button
      class="bg-red-600 hover:bg-red-700 text-white font-bold py-2 px-4 rounded shadow-md transition disabled:opacity-50"
      :disabled="isCleaning"
      @click="cleanDatabase"
    >
      <span v-if="isCleaning">Cleaning...</span>
      <span v-else>🧹 Clean Database</span>
    </button>
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