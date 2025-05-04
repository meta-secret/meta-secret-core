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
      jsAppState,
    };
  },

  data() {
    return {
      isLocalState: false,
      isMemberState: false,
      isVaultNotExists: false,
    };
  },

  async mounted() {
    this.isLocalState = await this.jsAppState.checkIsLocal();
    this.isMemberState = await this.jsAppState.checkIsMember();
    this.isVaultNotExists = await this.jsAppState.checkIsVaultNotExists();

    console.log('is in Local state: ', this.isLocalState);
    console.log('is in VaultNotExists state: ', this.isVaultNotExists);
    console.log('is in Member state: ', this.isMemberState);
  },
});
</script>

<template>
  <div :class="$style.titleContainer">
    <p :class="$style.titleText">Personal Secret Manager</p>
  </div>

  <div v-if="isLocalState || isVaultNotExists">
    <RegistrationComponent />
  </div>
  <div v-else-if="isMemberState">
    <VaultComponent />
  </div>
  <div v-else :class="$style.errorMessage">
    <h1>Outsider!</h1>
  </div>
</template>

<style module>
.titleContainer {
  @apply flex justify-center py-6;
}

.titleText {
  @apply text-2xl text-gray-900 dark:text-white;
}

.errorMessage {
  @apply text-gray-900 dark:text-white;
}
</style>
