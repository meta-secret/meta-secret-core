<script lang="ts">
import {defineComponent} from "vue";
import RegistrationComponent from "@/components/vault/Registration.vue";

import "@/common/DbUtils";
import {AppState} from "@/stores/app-state";
import init from "meta-secret-web-cli";

export default defineComponent({
  components: {
    RegistrationComponent,
  },

  async setup() {
    console.log("VaultView. Init");

    await init();

    const appState = AppState();
    await appState.appStateInit();

    return {
      appState: appState,
    };
  },

  methods: {
    isEmptyEnv() {
      return this.appState.internalState.metaVault === undefined;
    },

    getVaultName() {
      return this.appState.internalState.metaVault.vaultName;
    }
  }
});
</script>

<template>
  <div class="flex justify-center py-6">
    <p class="text-2xl">Distributed Password Manager</p>
  </div>

  <div v-if="this.isEmptyEnv()">
    <RegistrationComponent />
  </div>

  <div v-else>
    <div class="container flex justify-center max-w-md py-2 items-stretch">
      <p class="flex">{{ this.getVaultName() }}</p>
    </div>

    <div class="container flex max-w-md py-2 items-stretch">
      <RouterLink
        class="w-1/2 text-center rounded-l-lg px-6 py-3 text-white bg-orange-600 active:bg-orange-800"
        to="/vault/secrets"
        >Secrets
      </RouterLink>

      <RouterLink
        class="w-1/2 text-center rounded-r-lg px-6 py-3 text-dark bg-gray-100 active:bg-gray-300"
        to="/vault/devices"
        >Devices
      </RouterLink>
    </div>

    <div>
      <RouterView />
    </div>
  </div>
</template>