<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { MetaPasswordId, PlainPassInfo } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';

const appState = AppState();

const newPassword = ref('');
const newPassDescription = ref('');

const currentSecret = ref<any>(null);
const currentSecretId = ref<any>(null);

const showAddForm = ref(false);
const passwords = ref<MetaPasswordId[]>([]);

const loadPasswords = () => {
  passwords.value = appState.currState.as_vault().as_member().vault_data().secrets();
};

onMounted(() => {
  loadPasswords();
});

const addPassword = async () => {
  const pass = new PlainPassInfo(newPassDescription.value, newPassword.value);
  await appState.appManager.cluster_distribution(pass);
  newPassword.value = '';
  newPassDescription.value = '';
  showAddForm.value = false;
  await appState.updateState();
  loadPasswords();
};

const recover = async (metaPassId: MetaPasswordId) => {
  await appState.appManager.recover_js(metaPassId);
};

const showRecovered = async (metaPassId: MetaPasswordId) => {
  const id = metaPassId.id();
  if (currentSecretId.value === id) {
    currentSecret.value = null;
    currentSecretId.value = null;
    return;
  }
  currentSecret.value = await appState.appManager.show_recovered(metaPassId);
  currentSecretId.value = id;
};

const isRecovered = (metaPassId: MetaPasswordId) => {
  const maybeCompletedClaim = appState.currState.as_vault().as_member().find_recovery_claim(metaPassId);
  return maybeCompletedClaim !== undefined;
};

const toggleAddForm = () => {
  showAddForm.value = !showAddForm.value;
};
</script>

<template>
  <div :class="$style.spacer" />
  <!-- Secrets list with improved styling -->
  <div :class="$style.secretsContainer">
    <div :class="$style.secretsHeader">
      <h3 :class="$style.secretsTitle">Your Secrets</h3>
      <button :class="$style.addSecretButton" @click="toggleAddForm">
        <span>+ Add Secret</span>
      </button>
    </div>

    <div v-if="passwords.length === 0" :class="$style.emptyState">No secrets added yet</div>

    <ul v-else :class="$style.secretsList">
      <li v-for="secret in passwords" :key="secret.id()" :class="$style.secretListItem">
        <div :class="$style.secretHeader">
          <div :class="$style.secretInfo">
            <div :class="$style.secretName">
              {{ secret.name }}
            </div>
            <div :class="$style.secretId">ID: {{ secret.id() }}</div>
          </div>
          <div :class="$style.secretActions">
            <div v-if="isRecovered(secret)">
              <button :class="$style.showButton" @click="showRecovered(secret)">
                {{ currentSecretId === secret.id() ? 'Hide' : 'Show' }}
              </button>
            </div>
            <div v-else>
              <button :class="$style.recoveryButton" @click="recover(secret)">Recovery Request</button>
            </div>
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

  <div :class="$style.spacerLarge" />

  <!-- Modal overlay for the Add Secret form -->
  <div v-if="showAddForm" :class="$style.modalOverlay" @click.self="toggleAddForm">
    <div :class="$style.modalContainer">
      <div :class="$style.modalHeader">
        <h3 :class="$style.modalTitle">Add New Secret</h3>
        <button :class="$style.closeButton" @click="toggleAddForm">&times;</button>
      </div>

      <div :class="$style.modalBody">
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
    </div>
  </div>
</template>

<style module>
.spacer {
  @apply py-3;
}

.spacerLarge {
  @apply py-5;
}

.secretsList {
  @apply w-full flex flex-col;
}

.secretInfo {
  @apply flex-1 pl-1 mr-4;
}

.secretActions {
  @apply flex space-x-2;
}

.secretsTitle {
  @apply text-lg font-medium text-gray-800 dark:text-gray-200;
}

.secretsHeader {
  @apply flex justify-between items-center px-4 py-3;
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
  @apply focus-within:ring-2 focus-within:ring-slate-500 dark:focus-within:ring-slate-400;
  @apply focus-within:border-slate-500 dark:focus-within:border-slate-400;
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
  @apply bg-slate-600 hover:bg-slate-700 text-white font-medium py-2 px-5 rounded-md;
  @apply dark:bg-slate-700 dark:hover:bg-slate-800;
  @apply transition-colors duration-200;
  @apply disabled:opacity-50 disabled:cursor-not-allowed;
}

.secretsContainer {
  @apply container max-w-md mx-auto rounded-lg overflow-hidden;
  @apply bg-gray-50 dark:bg-gray-850;
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
  @apply bg-slate-100 dark:bg-slate-800;
  @apply border border-slate-300 dark:border-slate-600;
  @apply transition-all duration-300 ease-in-out;
}

.secretContent {
  @apply flex items-center;
}

.secretLabel {
  @apply font-medium text-slate-700 dark:text-slate-300 mr-2;
}

.secretValue {
  @apply font-mono text-gray-800 dark:text-gray-200;
  @apply bg-white dark:bg-gray-700 px-3 py-1.5 rounded-md;
  @apply border border-slate-300 dark:border-slate-600;
}

.addSecretButton {
  @apply bg-slate-100 hover:bg-slate-200 text-slate-700 font-medium py-2 px-4 rounded-md;
  @apply dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300;
  @apply border border-slate-300 dark:border-slate-600;
  @apply transition-colors duration-200;
}

.modalOverlay {
  @apply fixed inset-0 bg-black bg-opacity-50 dark:bg-opacity-70 flex items-center justify-center z-50;
  @apply transition-opacity duration-300 ease-in-out;
  @apply backdrop-blur-sm;
}

.modalContainer {
  @apply bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-md mx-4;
  @apply border border-gray-200 dark:border-gray-700;
  @apply transform transition-all duration-300 ease-in-out;
  @apply scale-100 opacity-100;
}

.modalHeader {
  @apply flex items-center justify-between px-6 py-4;
  @apply border-b border-gray-200 dark:border-gray-700;
}

.modalTitle {
  @apply text-lg font-medium text-gray-800 dark:text-gray-200;
}

.closeButton {
  @apply text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200;
  @apply text-2xl font-bold leading-none;
  @apply transition-colors duration-200;
}

.modalBody {
  @apply px-6 py-5;
}
</style>
