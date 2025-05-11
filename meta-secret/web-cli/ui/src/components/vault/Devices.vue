<script setup lang="ts">
import { AppState } from '@/stores/app-state';
import Device from '@/components/vault/Device.vue';
import { UserDataOutsiderStatus } from 'meta-secret-web-cli';

const appState = AppState();
// Use type assertion to avoid TypeScript error
const users = (appState.currState as any).as_vault().as_member().vault_data().users();

// Group devices by status
const memberDevices = users.filter(membership => membership.is_member());
const declinedDevices = users.filter(membership => {
  if (membership.is_outsider()) {
    const outsider = membership.as_outsider();
    return outsider.status === UserDataOutsiderStatus.Declined;
  }
  return false;
});
const pendingDevices = users.filter(membership => {
  if (membership.is_outsider()) {
    const outsider = membership.as_outsider();
    return outsider.status === UserDataOutsiderStatus.Pending;
  } else {
    return false;
  }
});
</script>

<template>
  <div :class="$style.spacer" />

  <!-- Main devices container -->
  <div :class="$style.devicesContainer">
    <h3 :class="$style.devicesTitle">Devices</h3>
    <p :class="$style.devicesDescription">Detailed information about user devices</p>

    <div v-if="users.length === 0" :class="$style.emptyState">No devices connected yet</div>

    <div v-else>
      <!-- Member devices section -->
      <div v-if="memberDevices.length > 0">
        <div :class="$style.sectionHeader">Members</div>
        <ul :class="$style.devicesList">
          <li
            v-for="membership in memberDevices"
            :key="membership.user_data().device.device_id.wasm_id_str()"
            :class="$style.deviceListItem"
          >
            <Device :membership="membership"/>
          </li>
        </ul>
      </div>

      <!-- Pending devices section -->
      <div v-if="pendingDevices.length > 0">
        <div :class="$style.sectionHeader">Pending Requests</div>
        <ul :class="$style.devicesList">
          <li
            v-for="membership in pendingDevices"
            :key="membership.user_data().device.device_id.wasm_id_str()"
            :class="$style.deviceListItem"
          >
            <Device :membership="membership"/>
          </li>
        </ul>
      </div>

      <!-- Declined devices section -->
      <div v-if="declinedDevices.length > 0">
        <div :class="$style.sectionHeader">Declined Devices</div>
        <ul :class="$style.devicesList">
          <li
            v-for="membership in declinedDevices"
            :key="membership.user_data().device.device_id.wasm_id_str()"
            :class="[$style.deviceListItem, $style.declinedListItem]"
          >
            <Device :membership="membership"/>
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>

<style module>
.spacer {
  @apply py-3;
}

.devicesList {
  @apply w-full flex flex-col;
}

.devicesContainer {
  @apply container max-w-md mx-auto rounded-lg overflow-hidden;
  @apply bg-white dark:bg-gray-850;
  @apply border border-gray-200 dark:border-gray-700 shadow-md;
}

.devicesTitle {
  @apply text-lg font-medium text-gray-800 dark:text-gray-200 px-4 pt-4 pb-1;
}

.devicesDescription {
  @apply max-w-2xl text-sm text-gray-500 dark:text-gray-300 px-4 pb-3;
  @apply border-b border-gray-200 dark:border-gray-700;
}

.emptyState {
  @apply py-6 text-center text-gray-500 dark:text-gray-400 italic;
}

.deviceListItem {
  @apply flex flex-col w-full transition-colors duration-200;
  @apply border-b border-gray-200 dark:border-gray-700 last:border-b-0;
  @apply hover:bg-orange-50 dark:hover:bg-gray-700;
}

.declinedListItem {
  @apply bg-gray-50 dark:bg-gray-800;
}

.sectionHeader {
  @apply text-base font-bold text-gray-700 dark:text-gray-200 px-4 py-3 bg-gray-50 dark:bg-gray-800;
  @apply border-t border-b border-gray-200 dark:border-gray-700;
  @apply uppercase tracking-wide;
}
</style>
