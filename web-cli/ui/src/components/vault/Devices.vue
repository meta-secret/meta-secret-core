<script lang="ts">
import init from "meta-secret-web-cli";
import {AppState} from "@/stores/app-state";
import Device from "@/components/vault/Device.vue";

export default {
  components: {Device},
  async setup() {
    console.log("Device component. Init")

    await init();

    const appState = AppState();

    return {
      appState: appState,
    };
  },
};
</script>

<template>
  <div class="py-4"/>

  <!-- https://www.tailwind-kit.com/components/list -->
  <div :class="$style.devices">
    <div :class="$style.listHeader">
      <h3 :class="$style.listTitle">Devices</h3>
      <p :class="$style.listDescription">
        Detailed information about user devices
      </p>
    </div>
    <ul class="w-full flex flex-col divide-y divide p-2">
      <li
          v-for="userSig in appState.internalState.vault?.signatures"
          :key="userSig.vault.device.deviceId"
          class="flex flex-row"
      >
        <Device :user-sig="userSig" sig-status="active"/>
      </li>

      <li
          v-for="userSig in appState.internalState.vault?.pending"
          :key="userSig.vault.device.deviceId"
          class="flex flex-row"
      >
        <Device :user-sig="userSig" sig-status="pending"/>
      </li>
    </ul>
  </div>
</template>

<style module>
.devices {
  @apply container max-w-md flex flex-col items-center justify-center w-full;
  @apply mx-auto bg-white rounded-lg shadow dark:bg-gray-800;
}

.actionButtonText {
  @apply flex justify-end w-24 text-right;
}

.listHeader {
  @apply w-full px-4 py-5 border-b sm:px-6;
}

.listTitle {
  @apply text-lg font-medium leading-6 text-gray-900 dark:text-white;
}

.listDescription {
  @apply max-w-2xl mt-1 text-sm text-gray-500 dark:text-gray-200;
}
</style>