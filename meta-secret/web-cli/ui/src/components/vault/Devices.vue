<script lang="ts">
import { defineComponent } from 'vue';
import { AppState } from '@/stores/app-state';
import Device from '@/components/vault/Device.vue';
import { WasmUserMembership } from '../../../pkg';

export default defineComponent({
  components: { Device },
  
  data() {
    return {
      appState: null as any,
      deviceList: [] as WasmUserMembership[]
    };
  },
  
  mounted() {
    console.log('Device component. Init');
    this.appState = AppState();
    this.refreshDevices();
  },

  methods: {
    refreshDevices() {
      try {
        if (this.appState && this.appState.metaSecretAppState) {
          this.deviceList = this.appState.metaSecretAppState.as_vault().as_member().vault_data().users();
        }
      } catch (e) {
        console.error("Error getting devices:", e);
        this.deviceList = [];
      }
    }
  }
});
</script>

<template>
  <div :class="$style.spacer" />

  <!-- Devices list with improved styling -->
  <div :class="$style.devicesContainer">
    <h3 :class="$style.devicesTitle">Devices</h3>
    <p :class="$style.devicesDescription">Detailed information about user devices</p>

    <div v-if="deviceList.length === 0" :class="$style.emptyState">No devices connected yet</div>

    <ul v-else :class="$style.devicesList">
      <li
        v-for="membership in deviceList"
        :key="membership.user_data().device.device_id.wasm_id_str()"
        :class="$style.deviceListItem"
      >
        <Device :membership="membership" sig-status="active" />
      </li>
    </ul>
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

.actionButtonText {
  @apply flex justify-end w-24 text-right;
}
</style>
