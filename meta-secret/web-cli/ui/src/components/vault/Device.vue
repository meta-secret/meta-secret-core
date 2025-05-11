<script setup lang="ts">
import {
  DeviceData,
  JoinActionUpdate,
  UserData,
  UserDataOutsiderStatus,
  WasmUserMembership,
} from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';

const props = defineProps<{ membership: WasmUserMembership }>();

const appState = AppState();

const getUser = (): UserData => {
  return props.membership.user_data();
};

const getDevice = (): DeviceData => {
  return props.membership.user_data().device;
};

const isMember = () => {
  return props.membership.is_member();
};

const isPending = () => {
  const isOutsider = props.membership.is_outsider();
  if (isOutsider) {
    const outsider = props.membership.as_outsider();
    return outsider.status === UserDataOutsiderStatus.Pending;
  } else {
    return false;
  }
};

const accept = async () => {
  const user = getUser();
  await appState.appManager.update_membership(user, JoinActionUpdate.Accept);
};

const decline = async () => {
  const user = getUser();
  await appState.appManager.update_membership(user, JoinActionUpdate.Decline);
};

</script>

<template>
  <div :class="$style.deviceContainer">
    <div :class="$style.deviceInfo">
      <div :class="$style.deviceName">
        {{ getDevice().device_name.as_str() }}
      </div>
      <div :class="$style.deviceId">ID: {{ getDevice().device_id.wasm_id_str() }}</div>
    </div>
    <div v-if="isMember()" :class="$style.statusBadge">Member</div>
    <div v-if="isPending()" :class="$style.actionButtons">
      <button :class="$style.acceptButton" @click="accept">Accept</button>
      <button :class="$style.declineButton" @click="decline">Decline</button>
    </div>
  </div>
</template>

<style module>
.deviceContainer {
  @apply flex items-center justify-between w-full py-4 px-5;
  @apply border-b border-gray-200 dark:border-gray-700 last:border-b-0;
  @apply transition-colors duration-200;
  @apply hover:bg-gray-100 dark:hover:bg-gray-750;
}

.deviceInfo {
  @apply flex-1 flex flex-col;
}

.deviceName {
  @apply font-medium text-gray-800 dark:text-white text-base;
}

.deviceId {
  @apply text-sm text-gray-600 dark:text-gray-400 mt-1;
}

.statusBadge {
  @apply inline-flex items-center justify-center px-2 py-1 mx-3;
  @apply text-xs font-bold leading-none text-green-100 bg-green-600 rounded-full;
}

.actionButtons {
  @apply flex space-x-2;
}

.acceptButton {
  @apply bg-green-500 hover:bg-green-600 text-white text-xs font-medium py-1 px-3 rounded-md;
  @apply transition-colors duration-150;
}

.declineButton {
  @apply bg-red-500 hover:bg-red-600 text-white text-xs font-medium py-1 px-3 rounded-md;
  @apply transition-colors duration-150;
}
</style>
