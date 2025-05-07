<script setup lang="ts">
import { ref, onMounted } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';

import { AppState } from '@/stores/app-state';
import init, { ApplicationStateInfo } from 'meta-secret-web-cli';

const jsAppState = ref<any>(null);
const state_info = ref<ApplicationStateInfo>(ApplicationStateInfo.Local);
const isInitialized = ref(false);

async function updateAppState() {
  if (!jsAppState.value) {
    return;
  }
  state_info.value = await jsAppState.value.stateInfo();
}

async function initializeApp() {
  await init();
  jsAppState.value = AppState();
  await jsAppState.value.appStateInit();
  await updateAppState();
  isInitialized.value = true;
}

onMounted(() => {
  initializeApp();
});
</script>

<template>
  <div class="flex justify-center py-6">
    <p class="text-2xl font-bold text-gray-900 dark:text-gray-100">Decentralized Secret Manager</p>
  </div>

  <div v-if="!isInitialized" class="text-center mt-8">
    <p class="text-gray-400">Loading Vault Information...</p>
  </div>

  <div v-else-if="state_info == ApplicationStateInfo.Local || state_info == ApplicationStateInfo.VaultNotExists">
    <RegistrationComponent />
  </div>
  <div v-else-if="state_info == ApplicationStateInfo.Member">
    <VaultComponent />
  </div>
  <div v-else-if="state_info == ApplicationStateInfo.Outsider">
    <div class="container mx-auto flex justify-center max-w-md pt-1 pb-4">
      <div class="text-gray-900 dark:text-white">
        <h1>Outsider!</h1>
      </div>
    </div>
  </div>
</template>
