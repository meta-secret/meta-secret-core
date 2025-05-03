<script lang="ts">
import { defineComponent } from 'vue';
import init, { MetaPasswordId, PlainPassInfo, WasmApplicationState } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';

export default defineComponent({
  data() {
    return {
      newPassword: '',
      newPassDescription: '',
      appState: null,
      currentSecret: null,
      currentSecretId: null,
    };
  },

  async mounted() {
    await init();
    this.appState = AppState();
  },

  methods: {
    async addPassword() {
      await init();
      const pass = new PlainPassInfo(this.newPassDescription, this.newPassword);
      await this.appState.appManager.cluster_distribution(pass);
    },

    async recover(metaPassId: MetaPasswordId) {
      await this.appState.appManager.recover_js(metaPassId);
    },

    async showRecovered(metaPassId: MetaPasswordId) {
      // Store the ID first to avoid null pointer issues
      const id = metaPassId.id();

      // Then get the secret
      const secret = await this.appState.appManager.show_recovered(metaPassId);

      // Update data properties directly
      this.currentSecret = secret;
      this.currentSecretId = id;
    },

    metaPasswords(): MetaPasswordId[] {
      if (!this.appState) return [];
      const msAppState: WasmApplicationState = this.appState.metaSecretAppState;
      return msAppState.as_vault().as_member().vault_data().secrets();
    },
  },
});
</script>

<template>
  <div class="py-2" />

  <div :class="$style.newPasswordDiv">
    <div class="flex items-center">
      <label class="text-gray-700 dark:text-gray-300 w-28">description: </label>
      <input type="text" :class="$style.passwordInput" placeholder="my meta secret" v-model="newPassDescription" />
    </div>
    <div class="flex items-center">
      <label class="text-gray-700 dark:text-gray-300 w-28">secret: </label>
      <input type="text" :class="$style.passwordInput" placeholder="top$ecret" v-model="newPassword" />
    </div>
    <div class="flex justify-end">
      <button :class="$style.addButton" @click="addPassword">Add</button>
    </div>
  </div>

  <div class="py-4" />

  <!-- https://www.tailwind-kit.com/components/list -->
  <div :class="$style.secrets">
    <ul class="w-full flex flex-col divide-y dark:divide-gray-700 p-2">
      <li v-for="secret in metaPasswords()" :key="secret.id()" class="flex flex-col">
        <div class="flex items-center flex-1 p-4 cursor-pointer select-none">
          <div class="flex-1 pl-1 mr-16">
            <div class="font-medium dark:text-white">
              {{ secret.name }}
            </div>
            <div class="text-sm text-gray-600 dark:text-gray-300">
              {{ secret.id() }}
            </div>
          </div>
          <div class="flex space-x-2">
            <button :class="$style.actionButtonText" @click="recover(secret)">Recovery Request</button>
            <button :class="$style.actionButtonText" @click="showRecovered(secret)">Show</button>
          </div>
        </div>
        
        <!-- Improved secret display -->
        <div
          v-if="currentSecretId === secret.id() && currentSecret"
          :class="$style.secretContainer"
        >
          <div :class="$style.secretContent">
            <span :class="$style.secretLabel">Secret:</span>
            <span :class="$style.secretValue">{{ currentSecret }}</span>
          </div>
        </div>
      </li>
    </ul>
  </div>
</template>

<style module>
.secrets {
  @apply container max-w-md flex flex-col items-center justify-center w-full;
  @apply mx-auto bg-white shadow dark:bg-gray-800 rounded-md;
}

.newPasswordDiv {
  @apply block max-w-md mx-auto items-center justify-center border-b border-t border-l border-r py-2 px-4;
  @apply bg-white dark:bg-gray-800 dark:border-gray-700 rounded-md shadow;
}

.passwordInput {
  @apply appearance-none bg-transparent border-none w-full text-gray-700 dark:text-gray-200 mr-3 py-1 px-2 leading-tight focus:outline-none;
}

.addButton {
  @apply flex-shrink-0 bg-orange-400 border-orange-500 text-sm border-2 text-white py-1 px-4 rounded;
  @apply hover:bg-orange-700 hover:border-orange-700 transition-colors duration-200;
}

.actionButtonText {
  @apply flex-shrink-0 bg-gray-700 border-gray-800 text-sm border-0 text-white py-2 px-4 rounded;
  @apply hover:bg-gray-900 hover:border-gray-900 transition-colors duration-200;
}

.secretContainer {
  @apply mx-4 mb-4 p-3 rounded-md;
  @apply bg-orange-50 dark:bg-gray-700 border border-orange-300 dark:border-gray-600;
  @apply transition-all duration-300 ease-in-out;
  @apply shadow-sm;
}

.secretContent {
  @apply flex items-center;
}

.secretLabel {
  @apply font-bold text-orange-700 dark:text-orange-300 mr-2;
}

.secretValue {
  @apply font-mono text-gray-800 dark:text-gray-200 bg-white dark:bg-gray-800 px-2 py-1 rounded;
  @apply border border-orange-200 dark:border-gray-600;
}
</style>
