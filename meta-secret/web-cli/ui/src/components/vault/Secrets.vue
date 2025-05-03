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
      // Clear inputs after adding
      this.newPassword = '';
      this.newPassDescription = '';
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
  <div class="py-3" />
  <!-- Secrets list with improved styling -->
  <div :class="$style.secretsContainer">
    <h3 :class="$style.secretsTitle">Your Secrets</h3>

    <div v-if="metaPasswords().length === 0" :class="$style.emptyState">No secrets added yet</div>

    <ul v-else class="w-full flex flex-col">
      <li v-for="secret in metaPasswords()" :key="secret.id()" :class="$style.secretListItem">
        <div :class="$style.secretHeader">
          <div class="flex-1 pl-1 mr-4">
            <div :class="$style.secretName">
              {{ secret.name }}
            </div>
            <div :class="$style.secretId">ID: {{ secret.id() }}</div>
          </div>
          <div class="flex space-x-2">
            <button :class="$style.recoveryButton" @click="recover(secret)">Recovery Request</button>
            <button :class="$style.showButton" @click="showRecovered(secret)">Show</button>
          </div>
        </div>

        <!-- Improved secret display -->
        <div v-if="currentSecretId === secret.id() && currentSecret" :class="$style.secretContainer">
          <div :class="$style.secretContent">
            <span :class="$style.secretLabel">Secret:</span>
            <span :class="$style.secretValue">{{ currentSecret }}</span>
          </div>
        </div>
      </li>
    </ul>
  </div>

  <div class="py-5" />

  <!-- Password input card -->
  <div :class="$style.newPasswordCard">
    <h3 :class="$style.cardTitle">Add New Secret</h3>

    <div :class="$style.inputGroup">
      <label :class="$style.inputLabel">Description</label>
      <div :class="$style.inputWrapper">
        <input type="text" :class="$style.input" placeholder="my meta secret" v-model="newPassDescription" />
      </div>
    </div>

    <div :class="$style.inputGroup">
      <label :class="$style.inputLabel">Secret</label>
      <div :class="$style.inputWrapper">
        <input type="password" :class="$style.input" placeholder="top$ecret" v-model="newPassword" />
      </div>
    </div>

    <div :class="$style.buttonContainer">
      <button :class="$style.addButton" @click="addPassword" :disabled="!newPassword || !newPassDescription">
        Add
      </button>
    </div>
  </div>

</template>

<style module>
.newPasswordCard {
  @apply block max-w-md mx-auto px-6 py-5;
  @apply bg-white dark:bg-gray-850 rounded-lg shadow-md;
  @apply border border-gray-200 dark:border-gray-700;
  @apply transition-all duration-200;
}

.cardTitle {
  @apply text-lg font-medium text-gray-800 dark:text-gray-200 mb-4;
  @apply border-b border-gray-200 dark:border-gray-700 pb-2;
}

.secretsTitle {
  @apply text-lg font-medium text-gray-800 dark:text-gray-200 px-4 py-3;
  @apply border-b border-gray-200 dark:border-gray-700;
}

.inputGroup {
  @apply mb-4;
}

.inputLabel {
  @apply block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1;
}

.inputWrapper {
  @apply relative rounded-md;
  @apply bg-gray-50 dark:bg-gray-800 border border-gray-300 dark:border-gray-600;
  @apply focus-within:ring-2 focus-within:ring-orange-400 dark:focus-within:ring-orange-500;
  @apply focus-within:border-orange-400 dark:focus-within:border-orange-500;
  @apply transition-all duration-200;
}

.input {
  @apply block w-full rounded-md py-2 px-3;
  @apply bg-transparent text-gray-700 dark:text-gray-200;
  @apply placeholder-gray-400 dark:placeholder-gray-500;
  @apply focus:outline-none;
}

.buttonContainer {
  @apply flex justify-end mt-5;
}

.addButton {
  @apply bg-orange-500 hover:bg-orange-600 text-white font-medium py-2 px-5 rounded-md;
  @apply dark:bg-orange-600 dark:hover:bg-orange-700;
  @apply transition-colors duration-200;
  @apply disabled:opacity-50 disabled:cursor-not-allowed;
}

.secretsContainer {
  @apply container max-w-md mx-auto rounded-lg overflow-hidden;
  @apply bg-white dark:bg-gray-850;
  @apply border border-gray-200 dark:border-gray-700 shadow-md;
}

.emptyState {
  @apply py-6 text-center text-gray-500 dark:text-gray-400 italic;
}

.secretListItem {
  @apply flex flex-col w-full transition-colors duration-200;
  @apply border-b border-gray-200 dark:border-gray-700 last:border-b-0;
  @apply hover:bg-gray-100 dark:hover:bg-gray-750;
}

.secretHeader {
  @apply flex items-center flex-1 p-4 cursor-pointer select-none;
}

.secretName {
  @apply font-medium text-gray-800 dark:text-gray-200;
}

.secretId {
  @apply text-xs text-gray-500 dark:text-gray-400 mt-1;
}

.recoveryButton {
  @apply bg-gray-600 hover:bg-gray-700 text-sm text-white py-1.5 px-3 rounded-md;
  @apply dark:bg-gray-700 dark:hover:bg-gray-600;
  @apply transition-colors duration-200;
}

.showButton {
  @apply bg-orange-500 hover:bg-orange-600 text-sm text-white py-1.5 px-3 rounded-md;
  @apply dark:bg-orange-600 dark:hover:bg-orange-700;
  @apply transition-colors duration-200;
}

.secretContainer {
  @apply mx-4 mb-4 p-3 rounded-md;
  @apply bg-orange-50 dark:bg-gray-750;
  @apply border border-orange-200 dark:border-gray-600;
  @apply transition-all duration-300 ease-in-out;
}

.secretContent {
  @apply flex items-center;
}

.secretLabel {
  @apply font-medium text-orange-700 dark:text-orange-300 mr-2;
}

.secretValue {
  @apply font-mono text-gray-800 dark:text-gray-200;
  @apply bg-white dark:bg-gray-750 px-3 py-1.5 rounded-md;
  @apply border border-orange-200 dark:border-gray-600;
}
</style>
