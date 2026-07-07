<script setup lang="ts">
import { ref, watch } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';
import { Skeleton } from '@/components/ui/skeleton';
import { useRoute } from 'vue-router';
import { useStatePolling } from '@/utils/statePolling';

const jsAppState = AppState();
const authStore = useAuthStore();
const route = useRoute();
const isInitialized = ref(false);
const statePoller = useStatePolling(() => jsAppState.updateState());

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
    } else {
      statePoller.stop();
      isInitialized.value = false;
    }
  },
  { immediate: true },
);

watch(
  () => isInitialized.value,
  (initialized) => {
    if (authStore.isAuthenticated && initialized) {
      statePoller.start();
      return;
    }
    statePoller.stop();
  },
  { immediate: true },
);

watch(
  () => route.path,
  () => {
    if (authStore.isAuthenticated && isInitialized.value) {
      void statePoller.refreshNow();
    }
  },
);
</script>

<template>
  <div
    v-if="authStore.isAuthenticated && !isInitialized"
    class="flex min-h-[calc(100vh-3.5rem)] items-center justify-center p-6"
  >
    <div class="w-full max-w-md space-y-3">
      <Skeleton class="h-4 w-48 mx-auto" />
      <Skeleton class="h-2 w-64 mx-auto" />
    </div>
  </div>

  <RegistrationComponent v-else-if="authStore.isAuthenticated && isInitialized && !jsAppState.isMember" />
  <VaultComponent v-else-if="authStore.isAuthenticated && isInitialized && jsAppState.isMember" />
</template>
