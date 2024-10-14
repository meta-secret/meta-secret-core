<script lang="ts">
import { defineComponent } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';

import { AppState } from '@/stores/app-state';
import init, { WasmWebAppStatus } from 'meta-secret-web-cli';

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
    isNewUser() {
      return this.appState.appState.is_new_user();
    },

    isLocalEnv() {
      return this.appState.appState.status() === WasmWebAppStatus.LocalEnv;
    },

    isMember() {
      return this.appState.appState.status() === WasmWebAppStatus.Member;
    },
  },
});
</script>

<template>
  <div class="flex justify-center py-6">
    <p class="text-2xl">Personal Secret Manager</p>
  </div>

  <div v-if="this.isNewUser()">
    <RegistrationComponent />
  </div>
  <div v-else-if="this.isMember()">
    <VaultComponent />
  </div>
  <div v-else>
    <h1>Another status: {{ this.appState.appState.status() }}</h1>
  </div>
</template>
