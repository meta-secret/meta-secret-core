<script setup lang="ts">
import { computed } from 'vue';
import LiveboardHeader from '@/components/liveboard/LiveboardHeader.vue';
import LiveboardHome from '@/components/liveboard/LiveboardHome.vue';
import { useLiveboardRouteSession } from '@/features/liveboard/routeContext';

const { session, controller, notices, runWrite } = useLiveboardRouteSession();
const view = computed(() => controller.value.state.value);
const removeDevice = () => runWrite((owner) => owner.devices.remove());
const decide = (accepted: boolean) => runWrite((owner) => owner.notifications.decide(accepted));
</script>

<template>
  <LiveboardHeader :vault-name="view.profile.vaultName" />
  <LiveboardHome
    :vault="view.vault"
    :secret-count="view.profile.secretCount"
    :notifications="notices"
    :notifications-ready="session.state.value.ready"
    :notification-receipt="view.notificationReceipt"
    :invite-open="view.inviteOpen"
    :can-create="view.canCreateSecret && session.state.value.ready"
    secrets-href="/secrets"
    profile-href="/profile"
    @select-device="controller.devices.preview($event)"
    @current="controller.devices.current()"
    @remove-device="removeDevice"
    @invite="controller.invite($event)"
    @create="controller.create.open()"
    @open-notice="controller.notifications.open()"
    @decide-notice="decide"
    @dismiss-error="session.dismissError()"
  />
</template>
