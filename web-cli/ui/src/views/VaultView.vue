<script lang="ts">
import { defineComponent } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';

import { AppState } from '@/stores/app-state';
import init from 'meta-secret-web-cli';

export default defineComponent({
  components: {
    RegistrationComponent,
    VaultComponent,
  },

  async setup() {
    console.log('VaultView. Init');

    await init();

    const appState = AppState();
    await appState.appStateInit();

    return {
      appState: appState,
    };
  },

  methods: {
    isLocalEnv() {
      return this.appState.is_local_env();
    },

    getVaultName() {
      return 'fake';
    },
  },
});
</script>

<template>
  <div class="flex justify-center py-6">
    <p class="text-2xl">Personal Secret Manager</p>
  </div>

  <div v-if="this.isLocalEnv()">
    <RegistrationComponent />
  </div>
  <div v-else>
    <VaultComponent />
  </div>
</template>
