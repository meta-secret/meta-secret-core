<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';
import { Skeleton } from '@/components/ui/skeleton';
import { useRoute } from 'vue-router';
import { createStateInvalidationController } from '@/utils/stateInvalidation';

const jsAppState = AppState();
const authStore = useAuthStore();
const route = useRoute();
const isInitialized = ref(false);
const vaultName = computed(() => jsAppState.getVaultName());
const stateInvalidation = createStateInvalidationController({
  refresh: () => jsAppState.updateState(),
});

onBeforeUnmount(() => stateInvalidation.disconnect());

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
      stateInvalidation.disconnect();
      isInitialized.value = false;
    }
  },
  { immediate: true },
);

watch(
  () => [authStore.isAuthenticated, isInitialized.value, vaultName.value] as const,
  ([isAuthenticated, initialized, currentVaultName]) => {
    if (isAuthenticated && initialized && currentVaultName) {
      stateInvalidation.connect(currentVaultName);
      return;
    }
    stateInvalidation.disconnect();
  },
  { immediate: true },
);

watch(
  () => route.path,
  () => {
    if (authStore.isAuthenticated && isInitialized.value) {
      void stateInvalidation.refreshNow();
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
