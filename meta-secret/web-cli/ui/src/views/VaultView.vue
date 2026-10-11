<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';
import { Skeleton } from '@/components/ui/skeleton';
import { useRoute } from 'vue-router';
import { createStateInvalidationController } from '@/utils/stateInvalidation';
import { createJoinResultController } from '@/utils/joinResult';
import { UserDataOutsiderStatus } from 'meta-secret-web-cli';
import { toast } from 'vue-sonner';

const jsAppState = AppState();
const authStore = useAuthStore();
const route = useRoute();
const isInitialized = ref(false);
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
  console.log('[StateEvents] syncing local events with server');
  const sync = manager.sync_now();
  serverSyncInFlight = sync.finally(() => {
    serverSyncInFlight = null;
  });
  return serverSyncInFlight;
};
const stateInvalidation = createStateInvalidationController({
  refresh: () => jsAppState.updateState(),
  syncBeforeRefresh: syncServerState,
  getAuthorization: () => jsAppState.getStateEventsAuthToken(vaultName.value),
});
const joinResult = createJoinResultController({
  refresh: () => jsAppState.updateState(),
  getAuthorization: () => jsAppState.getJoinResultAuthToken(),
  onError: (message, terminal) => {
    console.warn('[JoinResult] stream error:', message);
    if (terminal) {
      toast.error(
        'Live approval updates are unavailable. Refresh to reconnect; older join requests may need to be submitted again.',
      );
    }
  },
});
const isPendingJoin = computed(() => {
  if (!jsAppState.isOutsider || !jsAppState.currentState) return false;
  return jsAppState.currentState.as_vault().as_outsider().status === UserDataOutsiderStatus.Pending;
});

const reconnectJoinResultWhenVisible = () => {
  if (document.visibilityState !== 'visible' || !isPendingJoin.value || !vaultName.value) return;
  joinResult.disconnect();
  joinResult.connect(vaultName.value);
};

const handleBrowserOnline = async () => {
  if (!authStore.isAuthenticated || !isInitialized.value) return;
  console.log('[StateEvents] browser online; syncing local events and refreshing application state');
  try {
    await syncServerState();
    console.log('[StateEvents] reconnect sync_now finished');
  } catch (error) {
    console.warn('[StateEvents] reconnect sync deferred; background sync will retry', error);
  }
  await stateInvalidation.refreshNow();
};

window.addEventListener('online', handleBrowserOnline);
document.addEventListener('visibilitychange', reconnectJoinResultWhenVisible);

onBeforeUnmount(() => {
  window.removeEventListener('online', handleBrowserOnline);
  document.removeEventListener('visibilitychange', reconnectJoinResultWhenVisible);
  stateInvalidation.disconnect();
  joinResult.disconnect();
});

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
  () => [authStore.isAuthenticated, isInitialized.value, vaultName.value, isPendingJoin.value] as const,
  ([isAuthenticated, initialized, currentVaultName, pending]) => {
    if (isAuthenticated && initialized && pending && currentVaultName) {
      joinResult.connect(currentVaultName);
      return;
    }
    joinResult.disconnect();
  },
  { immediate: true },
);

watch(
  () => [authStore.isAuthenticated, isInitialized.value, vaultName.value, jsAppState.isMember] as const,
  ([isAuthenticated, initialized, currentVaultName, member]) => {
    if (isAuthenticated && initialized && member && currentVaultName) {
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
