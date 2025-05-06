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

  data() {
    return {
      jsAppState: null,
      isLocalState: false,
      isMemberState: false,
      isVaultNotExists: false,
      isInitialized: false
    };
  },

  created() {
    console.log('VaultView. Init');
    this.initializeApp();
  },

  methods: {
    async initializeApp() {
      try {
        await init();
        this.jsAppState = AppState();
        await this.jsAppState.appStateInit();
        this.isInitialized = true;
        await this.updateAppState();
      } catch (error) {
        console.error('Failed to initialize app:', error);
      }
    },

    async updateAppState() {
      if (!this.jsAppState) {
        console.warn('Attempted to update app state before initialization');
        return;
      }

      this.isLocalState = await this.jsAppState.checkIsLocal();
      this.isMemberState = await this.jsAppState.checkIsMember();
      this.isVaultNotExists = await this.jsAppState.checkIsVaultNotExists();

      console.log('is in Local state: ', this.isLocalState);
      console.log('is in VaultNotExists state: ', this.isVaultNotExists);
      console.log('is in Member state: ', this.isMemberState);
    },

    async handleStateChange() {
      console.log('Application state change detected');
      await this.updateAppState();
    }
  },
});
</script>

<template>
  <div class="flex justify-center py-6">
    <p class="text-2xl font-bold text-gray-900 dark:text-gray-100">Decentralized Secret Manager</p>
  </div>

  <div v-if="!isInitialized" class="text-center mt-8">
    <p class="text-gray-400">Initializing application...</p>
  </div>

  <div v-else-if="isLocalState || isVaultNotExists">
    <RegistrationComponent @state-changed="handleStateChange" />
  </div>
  <div v-else-if="isMemberState">
    <VaultComponent />
  </div>
  <div v-else class="container mx-auto flex justify-center max-w-md pt-1 pb-4">
    <div class="text-gray-900 dark:text-white">
      <h1>Outsider!</h1>
    </div>
  </div>
</template>

