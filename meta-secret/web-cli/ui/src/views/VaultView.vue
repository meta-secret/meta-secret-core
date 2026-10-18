<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import RegistrationComponent from '@/components/registration/OrbitRegistration.vue';
import VaultComponent from '@/components/vault/Vault.vue';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';
import { registrationSurfaceActive } from '@/features/registration/registrationSurface';
import { Skeleton } from '@/components/ui/skeleton';
import { Button } from '@/components/ui/button';
import { useRoute } from 'vue-router';
import { createStateInvalidationController } from '@/utils/stateInvalidation';

const jsAppState = AppState();
const authStore = useAuthStore();
const route = useRoute();
const isInitialized = ref(false);
let disposed = false;
const registrationActive = registrationSurfaceActive;
const vaultName = computed(() => jsAppState.getVaultName());
let serverSyncInFlight: Promise<void> | null = null;
const syncServerState = () => {
  if (serverSyncInFlight) return serverSyncInFlight;
  const manager = jsAppState.appManager as typeof jsAppState.appManager & {
    sync_now?: () => Promise<void>;
  };
  if (typeof manager?.sync_now !== 'function') {
    console.warn('[StateEvents] sync_now is unavailable in the loaded WASM module');
    return Promise.resolve();
  }
  jsAppState.beginSync();
  const sync = Promise.resolve()
    .then(() => manager.sync_now!())
    .catch((error) => {
      jsAppState.syncError = 'Could not synchronize Vault. Retry to apply pending changes.';
      throw error;
    });
  serverSyncInFlight = sync.finally(() => {
    jsAppState.endSync();
    serverSyncInFlight = null;
  });
  return serverSyncInFlight;
};
const stateInvalidation = createStateInvalidationController({
  refresh: () => jsAppState.updateState(),
  syncBeforeRefresh: syncServerState,
  getAuthorization: () => jsAppState.getStateEventsAuthToken(vaultName.value),
});
const handleBrowserOnline = async () => {
  if (!authStore.isAuthenticated || !isInitialized.value || registrationActive.value) return;
  try {
    await syncServerState();
  } catch (error) {
    console.warn('[StateEvents] reconnect sync deferred; background sync will retry', error);
  }
  await stateInvalidation.refreshNow();
};

window.addEventListener('online', handleBrowserOnline);

onBeforeUnmount(() => {
  disposed = true;
  registrationActive.value = false;
  window.removeEventListener('online', handleBrowserOnline);

  stateInvalidation.disconnect();
});

watch(
  () => authStore.isAuthenticated,
  async (isAuthenticated) => {
    if (isAuthenticated && authStore.masterKey) {
      try {
        await jsAppState.appStateInit();
        if (disposed) return;
        registrationActive.value = !jsAppState.isMember;
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
  () => jsAppState.isMember,
  (member, wasMember) => {
    if (isInitialized.value && wasMember && !member) registrationActive.value = true;
  },
);

watch(
  () =>
    [
      authStore.isAuthenticated,
      isInitialized.value,
      vaultName.value,
      jsAppState.isMember,
      registrationActive.value,
    ] as const,
  ([isAuthenticated, initialized, currentVaultName, member, registering]) => {
    if (isAuthenticated && initialized && member && currentVaultName && !registering) {
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
    if (authStore.isAuthenticated && isInitialized.value && !registrationActive.value) {
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

  <RegistrationComponent
    v-else-if="authStore.isAuthenticated && isInitialized && registrationActive"
    @ready="registrationActive = false"
  />
  <VaultComponent
    v-else-if="authStore.isAuthenticated && isInitialized && !registrationActive && jsAppState.isMember"
    v-show="!jsAppState.isSynchronizing && !jsAppState.syncError"
  />
  <div
    v-if="
      authStore.isAuthenticated &&
      isInitialized &&
      !registrationActive &&
      (jsAppState.isSynchronizing || jsAppState.syncError)
    "
    role="status"
    aria-live="polite"
    aria-busy="true"
    data-testid="vault-sync-loader"
    class="fixed inset-0 z-[100] flex flex-col items-center justify-center gap-4 bg-background p-6"
  >
    <Skeleton v-if="jsAppState.isSynchronizing" class="h-4 w-48" />
    <p v-if="jsAppState.syncError" role="alert">{{ jsAppState.syncError }}</p>
    <p v-else>Synchronizing Vault and applying pending changes…</p>
    <Button v-if="jsAppState.syncError" :disabled="jsAppState.isSynchronizing" @click="stateInvalidation.refreshNow()"
      >Retry</Button
    >
  </div>
</template>
