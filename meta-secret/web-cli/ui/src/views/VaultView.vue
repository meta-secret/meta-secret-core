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

    const jsAppState = AppState();
    await jsAppState.appStateInit();

    return {
      jsAppState: jsAppState,
    };
  },

  methods: {
    async isLocal() {
      const currState = await this.jsAppState.appManager.get_state();
      return currState.is_local();
    },

    async isMember() {
      const currState = await this.jsAppState.appManager.get_state();
      const isVault = currState.is_vault();
      if (!isVault) {
        return false;
      }

      return this.metaSecretAppState.jsAppState.as_vault().is_member();
    },
  },
});
</script>

<template>
  <div class="flex justify-center py-6">
    <p class="text-2xl">Personal Secret Manager</p>
  </div>

  <div v-if="this.isLocal()">
    <RegistrationComponent />
  </div>
  <div v-else-if="this.isMember()">
    <VaultComponent />
  </div>
  <div v-else>
    <h1>Outsider!</h1>
  </div>
</template>
