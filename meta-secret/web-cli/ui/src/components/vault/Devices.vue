<script lang="ts">
import { AppState } from '@/stores/app-state';
import Device from '@/components/vault/Device.vue';
import { WasmUserMembership } from '../../../pkg';

export default {
  components: { Device },
  async setup() {
    console.log('Device component. Init');

    const appState = AppState();

    return {
      appState: appState,
    };
  },

  methods: {
    users(): WasmUserMembership[] {
      return this.appState.metaSecretAppState.as_vault().as_member().vault_data().users();
    },
  },
};
</script>

<template>
  <div class="py-4" />

  <!-- https://www.tailwind-kit.com/components/list -->
  <div :class="$style.devices">
    <div :class="$style.listHeader">
      <h3 :class="$style.listTitle">Devices</h3>
      <p :class="$style.listDescription">Detailed information about user devices</p>
    </div>
    <ul class="w-full flex flex-col divide-y divide p-2 dark:divide-gray-700">
      <li
        v-for="membership in this.users()"
        :key="membership.user_data().device.device_id.wasm_id_str()"
        class="flex flex-row"
      >
        <Device :membership="membership" sig-status="active" />
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
  @apply w-full px-4 py-5 border-b sm:px-6 dark:border-gray-700;
}

.listTitle {
  @apply text-lg font-medium leading-6 text-gray-900 dark:text-white;
}

.listDescription {
  @apply max-w-2xl mt-1 text-sm text-gray-500 dark:text-gray-200;
}
</style>
