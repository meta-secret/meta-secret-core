<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from 'vue';
import { useRoute } from 'vue-router';
import RegistrationComponent from '@/components/registration/OrbitRegistration.vue';
import LiveboardShell from '@/components/liveboard/LiveboardShell.vue';
import InviteQrDialog from '@/components/liveboard/InviteQrDialog.vue';
import CreateSecretDialog from '@/components/liveboard/CreateSecretDialog.vue';
import ResetLocalDialog from '@/components/liveboard/ResetLocalDialog.vue';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';
import { registrationSurfaceActive } from '@/features/registration/registrationSurface';
import { useVaultSession } from '@/features/liveboard/useVaultSession';
import { provideLiveboardRouteSession } from '@/features/liveboard/routeContext';
import type { LiveboardRoute } from '@/features/liveboard/controller';
import { Skeleton } from '@/components/ui/skeleton';

const jsAppState = AppState();
const authStore = useAuthStore();
const route = useRoute();
const session = useVaultSession({ store: jsAppState, auth: authStore });
const { controller, runWrite } = provideLiveboardRouteSession(session);
const registrationActive = registrationSurfaceActive;
const view = computed(() => controller.value.state.value);
const inviteAnchor = computed(() => {
  const anchor = view.value.inviteAnchor;
  return anchor instanceof HTMLElement ? anchor : null;
});
const creation = computed(() => controller.value.create.state.value);
const routeIntent = computed<LiveboardRoute>(() =>
  route.name === 'homeSecrets' ? 'secrets' : route.name === 'homeProfile' ? 'profile' : 'home',
);
const memberSurface = computed(
  () =>
    authStore.isAuthenticated &&
    !registrationActive.value &&
    ((session.state.value.initialized && jsAppState.isMember && session.state.value.member) ||
      session.state.value.resetting),
);

watch(
  () =>
    [
      authStore.isAuthenticated,
      session.state.value.initialized,
      jsAppState.isMember,
      session.state.value.resetting,
    ] as const,
  ([authenticated, initialized, member, resetting]) => {
    if (authenticated && initialized && !resetting && !member) registrationActive.value = true;
  },
  { immediate: true, flush: 'sync' },
);
watch(
  () => [routeIntent.value, session.controller] as const,
  ([intent]) => {
    session.controller.navigate(intent);
    if (session.state.value.ready && !registrationActive.value) void session.refresh().catch(() => undefined);
  },
  { immediate: true, flush: 'sync' },
);
function registrationReady() {
  if (jsAppState.isMember) registrationActive.value = false;
}
const createSecret = () => runWrite((owner) => owner.create.submit());
onBeforeUnmount(() => {
  registrationActive.value = false;
});
</script>

<template>
  <div
    v-if="authStore.isAuthenticated && !session.state.value.initialized && !session.state.value.resetting"
    class="flex min-h-[calc(100vh-3.5rem)] items-center justify-center p-6"
    role="status"
    aria-label="Initializing Vault"
    aria-busy="true"
  >
    <div class="w-full max-w-md space-y-3">
      <Skeleton class="h-4 w-48 mx-auto" />
      <Skeleton class="h-2 w-64 mx-auto" />
    </div>
  </div>
  <RegistrationComponent
    v-else-if="authStore.isAuthenticated && session.state.value.initialized && registrationActive"
    @ready="registrationReady"
  />
  <LiveboardShell v-else-if="memberSurface">
    <RouterView />
    <template #dialogs>
      <InviteQrDialog
        :open="view.inviteOpen"
        :anchor="inviteAnchor"
        :vault-name="view.profile.vaultName"
        @close="controller.closeInvite()"
        @error="controller.showError($event)"
      />
      <CreateSecretDialog
        :view="creation"
        :blocked="!session.state.value.ready || !view.canCreateSecret"
        @change="controller.create.change($event)"
        @paste="controller.create.paste()"
        @paste-text="controller.create.pasteText($event)"
        @submit="createSecret"
        @cancel="controller.create.cancel()"
      />
      <ResetLocalDialog
        :open="view.resetOpen"
        :busy="view.resetBusy"
        @cancel="controller.cancelReset()"
        @confirm="controller.confirmReset()"
      />
    </template>
  </LiveboardShell>
</template>
