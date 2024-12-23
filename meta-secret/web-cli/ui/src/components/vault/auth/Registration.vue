<script lang="ts">
import { defineComponent } from 'vue';
import { AppState } from '@/stores/app-state';
import { WasmApplicationState } from '../../../../pkg';

export default defineComponent({
  async setup() {
    console.log('JS: Registration component. Init');

    const appState = AppState();
    return {
      appState: appState,
      vaultName: '',
    };
  },

  methods: {
    async registration() {
      console.log('Generate vault');
      await this.appState.appManager.sign_up(this.vaultName);
    },

    isMember() {
      const isVault = this.appState.metaSecretAppState.is_vault();
      if (!isVault) {
        return false;
      }

      return this.metaSecretAppState.appState.as_vault().is_member();
    },

    isNonMember() {
      return !this.isMember();
    },

    isLocalEnv() {
      const appState: WasmApplicationState = this.appState.metaSecretAppState;
      if (!appState) {
        console.log('isEmptyEnv: appState is not initialized');
        throw new Error('Invalid environment');
      }

      return appState.is_local();
    },

    isNewUser() {
      const appState: WasmApplicationState = this.appState.metaSecretAppState;
      return appState.is_new_user();
    },
  },
});
</script>

<template>
  <div v-if="this.isLocalEnv()">
    <div class="container flex items-center max-w-md py-2">
      <label>User:</label>
    </div>

    <div class="container flex items-center justify-center max-w-md border-b border-t border-l border-r py-2 px-2">
      <label>@</label>
      <input :class="$style.nicknameUserInput" type="text" placeholder="vault name" v-model="vaultName" />

      <button v-if="this.isNewUser()" :class="$style.registrationButton" @click="registration">Register</button>
    </div>
  </div>

  <div v-if="this.isNonMember()">
    <div class="container flex items-center max-w-md py-2">
      <label :class="$style.joinLabel">Vault already exists, would you like to join?</label>
      <button :class="$style.joinButton" @click="registration">Join</button>
    </div>
  </div>
</template>

<style module>
.joinLabel {
  @apply appearance-none bg-transparent border-none w-full text-gray-700 mr-3 py-1 leading-tight focus:outline-none;
}

.registrationButton {
  @apply flex-shrink-0 bg-teal-500 border-teal-500 text-sm border-4 text-white py-1 px-4 rounded;
  @apply hover:bg-teal-700 hover:border-teal-700;
}

.joinButton {
  @apply flex-shrink-0 bg-teal-500;
  @apply hover:bg-teal-700 border-teal-500 hover:border-teal-700 text-sm border-4 text-white py-1 px-4 rounded;
}

.nicknameUserInput {
  @apply appearance-none bg-transparent border-none;
  @apply w-full text-gray-700 mr-3 py-1 px-2 leading-tight focus:outline-none;
}
</style>
