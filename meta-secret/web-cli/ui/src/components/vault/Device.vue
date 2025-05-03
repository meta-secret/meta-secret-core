<script lang="ts">
import type { PropType } from 'vue';
import { defineComponent } from 'vue';
import { DeviceData, UserDataOutsider, UserDataOutsiderStatus, WasmUserMembership } from '../../../pkg';

export default defineComponent({
  props: {
    membership: Object as PropType<WasmUserMembership>,
  },

  methods: {
    async accept() {
      //await this.membership(deviceInfo, MembershipRequestType.Accept);
    },

    async decline() {
      //await this.membership(deviceInfo, MembershipRequestType.Decline);
    },

    getDevice(): DeviceData {
      return this.membership.user_data().device;
    },

    isPending() {
      const isOutsider = this.membership.is_outsider();
      if (isOutsider) {
        const outsider: UserDataOutsider = this.membership.as_outsider();
        return outsider.status === UserDataOutsiderStatus.Pending;
      } else {
        return false;
      }
    },
  },
});
</script>

<template>
  <div :class="$style.deviceContainer">
    <div :class="$style.deviceInfo">
      <div :class="$style.deviceName">
        {{ this.getDevice().device_name.as_str() || "Device" }}
      </div>
      <div :class="$style.deviceId">
        ID: {{ this.getDevice().device_id.wasm_id_str() }}
      </div>
    </div>
    <div :class="$style.statusBadge">
      Active
    </div>
    <div v-if="this.isPending()" :class="$style.actionButtons">
      <button :class="$style.acceptButton" @click="accept()">Accept</button>
      <button :class="$style.declineButton" @click="decline()">Decline</button>
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

