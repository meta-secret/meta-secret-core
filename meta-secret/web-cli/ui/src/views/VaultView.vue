<script setup lang="ts">
import { ref, watch } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';

const jsAppState = AppState();
const authStore = useAuthStore();
const isInitialized = ref(false);

watch(
  () => authStore.isAuthenticated,
  async (isAuthenticated) => {
    if (isAuthenticated && authStore.masterKey) {
      try {
        await jsAppState.appStateInit();
        isInitialized.value = true;
      } catch (error) {
        console.error('Error initializing app state:', error);
      }
    }
  },
  { immediate: true },
);
</script>

<template>
  <div v-if="authStore.isAuthenticated && !isInitialized" class="loading-wrap">
    <p class="loading-text">Loading Vault Information...</p>
  </div>

  <div v-else-if="authStore.isAuthenticated && isInitialized && !jsAppState.isMember">
    <RegistrationComponent />
  </div>

  <div v-else-if="authStore.isAuthenticated && isInitialized && jsAppState.isMember">
    <VaultComponent />
  </div>
</template>

<style scoped>
.loading-wrap {
  min-height: calc(100vh - 60px);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}

.loading-text {
  color: #8aaacf;
  font-size: 15px;
}
</style>
