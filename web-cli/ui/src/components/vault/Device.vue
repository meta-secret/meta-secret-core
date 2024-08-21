<script lang="ts">

import type {PropType} from 'vue'
import {defineComponent} from 'vue'
import init from "meta-secret-web-cli";
import type {UserSignature} from "@/model/UserSignature";

export default defineComponent({
  props: {
    userSig: Object as PropType<UserSignature>,
    sigStatus: String
  },

  methods: {
    async accept() {
      await init();
      //await this.membership(deviceInfo, MembershipRequestType.Accept);
    },

    async decline() {
      await init();
      //await this.membership(deviceInfo, MembershipRequestType.Decline);
    },

    /*
    async membership(
        deviceInfo: DeviceUiElement,
        requestType: MembershipRequestType
    ) {
      let membershipResult = membership(deviceInfo.userSig, requestType);
      console.log("membership operation: ", membershipResult);
      //TODO check the operation status

      await router.push({path: "/vault/devices"});
    },
     */
  },
});

</script>

<template>
  <div class="flex items-center flex-1 p-4 cursor-pointer select-none">
    <div class="flex-1 pl-1 mr-16">
      <div class="font-medium dark:text-white">
        {{ userSig.vault.device.deviceName }}
      </div>
      <div class="text-sm text-gray-600 dark:text-gray-200 truncate">
        <p class="truncate w-24">
          {{ userSig.vault.device.deviceId }}
        </p>
      </div>
    </div>
    <div class="text-xs text-gray-600 dark:text-gray-200">
      Active
    </div>
    <button
        v-if="sigStatus === 'pending'"
        :class="$style.actionButtonText"
        @click="accept()"
    >
      Accept
    </button>
    <button
        v-if="sigStatus === 'pending'"
        :class="$style.actionButtonText"
        @click="decline()"
    >
      Decline
    </button>
  </div>
</template>


<style module>
.actionButtonText {
  @apply flex justify-end w-24 text-right;
}
</style>