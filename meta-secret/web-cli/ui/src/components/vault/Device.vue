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
  <div class="flex items-center flex-1 p-4 cursor-pointer select-none">
    <div class="flex-1 pl-1 mr-16">
      <div class="font-medium dark:text-white">
        {{ this.getDevice().device_name.as_str() }}
      </div>
      <div class="text-sm text-gray-600 dark:text-gray-200 truncate">
        <p class="truncate w-24">
          {{ this.getDevice().device_id.as_str() }}
        </p>
      </div>
    </div>
    <div class="text-xs text-gray-600 dark:text-gray-200">Active</div>
    <button v-if="this.isPending()" :class="$style.actionButtonText" @click="accept()">Accept</button>
    <button v-if="this.isPending()" :class="$style.actionButtonText" @click="decline()">Decline</button>
  </div>
</template>

<style module>
.actionButtonText {
  @apply flex justify-end w-24 text-right;
}
</style>
