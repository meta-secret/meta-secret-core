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

  <!-- Secrets list with improved styling -->
  <div :class="$style.secrets">
    <ul class="w-full flex flex-col">
      <li v-for="secret in metaPasswords()" :key="secret.id()" :class="$style.secretListItem">
        <div :class="$style.secretHeader">
          <div class="flex-1 pl-1 mr-16">
            <div :class="$style.secretName">
              {{ secret.name }}
            </div>
            <div :class="$style.secretId">
              {{ secret.id() }}
            </div>
          </div>
          <div class="flex space-x-2">
            <button :class="$style.actionButtonText" @click="recover(secret)">Recovery Request</button>
            <button :class="$style.showButton" @click="showRecovered(secret)">Show</button>
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
  @apply mx-auto bg-gray-50 shadow-md dark:bg-gray-800 rounded-md overflow-hidden;
  @apply border border-gray-200 dark:border-gray-700;
}

.newPasswordDiv {
  @apply block max-w-md mx-auto items-center justify-center py-3 px-5;
  @apply bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-md;
}

.passwordInput {
  @apply appearance-none bg-transparent border-none w-full text-gray-700 dark:text-gray-200 mr-3 py-1 px-2 leading-tight focus:outline-none;
}

.addButton {
  @apply flex-shrink-0 bg-orange-400 border-orange-500 text-sm border-2 text-white py-1 px-4 rounded;
  @apply hover:bg-orange-500 hover:border-orange-600 transition-colors duration-200;
}

.secretListItem {
  @apply flex flex-col w-full transition-colors duration-200;
  @apply border-b border-gray-200 dark:border-gray-700 last:border-b-0;
  @apply bg-white dark:bg-gray-800;
  @apply hover:bg-orange-50 dark:hover:bg-gray-700;
}

.secretHeader {
  @apply flex items-center flex-1 p-4 cursor-pointer select-none;
}

.secretName {
  @apply font-medium text-gray-800 dark:text-gray-200 text-lg;
}

.secretId {
  @apply text-sm text-gray-500 dark:text-gray-400 mt-1;
}

.actionButtonText {
  @apply flex-shrink-0 bg-gray-700 text-sm text-white py-2 px-4 rounded;
  @apply hover:bg-gray-800 transition-colors duration-200;
  @apply dark:bg-gray-600 dark:hover:bg-gray-700 dark:text-gray-100;
}

.showButton {
  @apply flex-shrink-0 bg-orange-500 text-sm text-white py-2 px-4 rounded;
  @apply hover:bg-orange-600 transition-colors duration-200;
  @apply dark:bg-orange-600 dark:hover:bg-orange-700 dark:text-white;
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
