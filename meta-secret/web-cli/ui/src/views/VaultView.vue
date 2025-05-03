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

  data() {
    return {
      isLocalState: false,
      isMemberState: false,
    };
  },

  async mounted() {
    await this.checkIsLocal();
    await this.checkIsMember();
  },

  methods: {
    async checkIsLocal() {
      const currState = await this.jsAppState.appManager.get_state();
      this.isLocalState = currState.is_local();
      console.log('is in Local state: ', this.isLocalState);
    },

    async checkIsMember() {
      const currState = await this.jsAppState.appManager.get_state();
      const isVault = currState.is_vault();
      
      if (!isVault) {
        this.isMemberState = false;
        return;
      }

      const vaultState = currState.as_vault();
      this.isMemberState = vaultState.is_member();
      console.log('is in Member state: ', this.isMemberState);
    },

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

      return currState.as_vault().is_member();
    },
  },
});
</script>

<template>
  <div class="flex justify-center py-6">
    <p class="text-2xl">Personal Secret Manager</p>
  </div>

  <div v-if="isLocalState">
    <RegistrationComponent />
  </div>
  <div v-else-if="isMemberState">
    <VaultComponent />
  </div>
  <div v-else>
    <h1>Outsider!</h1>
  </div>
</template>
