<script setup lang="ts">
import { ref, computed } from 'vue';
import { MetaPasswordId, PlainPassInfo } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { vaultSecrets } from '@/locales/en';
import AddSecretForm from './AddSecretForm.vue';

const appState = AppState();
// Treat appManager as any type to avoid TypeScript errors
const appManager = appState.appManager as any;

const currentSecret = ref<any>(null);
const currentSecretId = ref<any>(null);
const copySuccess = ref<string | null>(null);
const showRecoveredError = ref<string | null>(null);
const copySecretError = ref<string | null>(null);
const loadingRecovery = ref<string | null>(null); // Track which secret is being recovered
const loadingShow = ref<string | null>(null); // Track which secret is being shown
const loadingCopy = ref<string | null>(null); // Track which secret is being copied

const showAddForm = ref(false);
const passwords = computed(() => appState.passwords);

const recover = async (metaPassId: MetaPasswordId) => {
  const id = metaPassId.id_str();
  loadingRecovery.value = id; // Set loading state for this specific secret
  try {
    await appManager.recover_js(metaPassId);
    await appState.updateState();
  } finally {
    loadingRecovery.value = null; // Clear loading state regardless of outcome
  }
};

const showRecovered = async (metaPassId: MetaPasswordId) => {
  const id = metaPassId.id_str();
  if (currentSecretId.value === id) {
    currentSecret.value = null;
    currentSecretId.value = null;
    showRecoveredError.value = null;
    return;
  }
  showRecoveredError.value = null;
  loadingShow.value = id; // Set loading state
  try {
    currentSecret.value = await appManager.show_recovered(metaPassId);
    currentSecretId.value = id;
  } catch (e) {
    console.error('show_recovered failed', e);
    showRecoveredError.value = vaultSecrets.errorShowRecovered;
    currentSecret.value = null;
    currentSecretId.value = null;
  } finally {
    loadingShow.value = null; // Clear loading state
  }
};

const copyToClipboard = async (metaPassId: MetaPasswordId) => {
  const id = metaPassId.id_str();
  copySecretError.value = null;
  try {
    loadingCopy.value = id; // Set loading state
    const secretText = await appManager.show_recovered(metaPassId);
    await navigator.clipboard.writeText(secretText);
    copySuccess.value = id;
    setTimeout(() => {
      if (copySuccess.value === id) {
        copySuccess.value = null;
      }
    }, 5000);
  } catch (err) {
    console.error('Failed to copy: ', err);
    copySecretError.value = vaultSecrets.errorCopySecret;
  } finally {
    loadingCopy.value = null; // Clear loading state
  }
};

const isRecovered = (metaPassId: MetaPasswordId) => {
  // Safely access properties using optional chaining
  const maybeCompletedClaim = (appState.currState as any).as_vault?.()?.as_member?.()?.find_recovery_claim(metaPassId);
  return maybeCompletedClaim !== undefined;
};

const toggleAddForm = () => {
  showAddForm.value = !showAddForm.value;
};

const handleSecretAdded = () => {
  showAddForm.value = false; // Just close the form without toggling
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

    <template v-else>
      <div
        v-if="showRecoveredError || copySecretError"
        :class="$style.inlineError"
        role="alert"
      >
        {{ showRecoveredError || copySecretError }}
      </div>

      <ul :class="$style.secretsList">
      <li v-for="secret in passwords" :key="secret.id_str()" :class="$style.secretListItem">
        <div :class="$style.secretHeader">
          <div :class="$style.secretInfo">
            <div :class="$style.secretName">
              {{ secret.name }}
            </div>
            <div :class="$style.secretId">ID: {{ secret.id_str() }}</div>
          </div>
          <div :class="$style.secretActions">
            <div v-if="isRecovered(secret)" :class="$style.buttonGroup">
              <button 
                :class="loadingShow === secret.id_str() ? [$style.showButton, $style.loading] : $style.showButton" 
                @click="showRecovered(secret)"
                :disabled="loadingShow === secret.id_str()"
              >
                <span v-if="loadingShow === secret.id_str()" :class="$style.spinner"></span>
                {{ currentSecretId === secret.id_str() ? 'Hide' : (loadingShow === secret.id_str() ? 'Loading...' : 'Show') }}
              </button>
              <button 
                :class="loadingCopy === secret.id_str() || copySuccess === secret.id_str() ? [$style.copyButton, copySuccess === secret.id_str() ? $style.success : $style.loading] : $style.copyButton" 
                @click="copyToClipboard(secret)"
                :disabled="loadingCopy === secret.id_str()"
              >
                <span v-if="loadingCopy === secret.id_str()" :class="$style.spinner"></span>
                {{ copySuccess === secret.id_str() ? 'Copied!' : (loadingCopy === secret.id_str() ? 'Copying...' : 'Copy') }}
              </button>
            </div>
            <div v-else>
              <button 
                :class="loadingRecovery === secret.id_str() ? [$style.recoveryButton, $style.loading] : $style.recoveryButton" 
                @click="recover(secret)" 
                :disabled="loadingRecovery === secret.id_str()"
              >
                <span v-if="loadingRecovery === secret.id_str()" :class="$style.spinner"></span>
                <span>{{ loadingRecovery === secret.id_str() ? 'Processing...' : 'Recovery Request' }}</span>
              </button>
            </div>
          </div>
        </div>

        <!-- Improved secret display -->
        <div v-if="currentSecretId === secret.id_str() && currentSecret" :class="$style.secretContainer">
          <div :class="$style.secretContent">
            <span :class="$style.secretLabel">Secret:</span>
            <span :class="$style.secretValue">{{ currentSecret }}</span>
          </div>
        </div>
      </li>
    </ul>
    </template>
  </div>

  <div :class="$style.spacerLarge" />

  <AddSecretForm :show="showAddForm" @added="handleSecretAdded" @close="toggleAddForm" />
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
  @apply flex space-x-4;
}

.secretsTitle {
  @apply text-lg font-medium text-gray-800 dark:text-gray-200;
}

.secretsHeader {
  @apply flex justify-between items-center px-4 py-3;
  @apply border-b border-gray-200 dark:border-gray-700;
}

.secretsContainer {
  @apply container max-w-md mx-auto rounded-lg overflow-hidden;
  @apply bg-gray-50 dark:bg-gray-850;
  @apply border border-gray-200 dark:border-gray-700 shadow-md;
}

.emptyState {
  @apply py-6 text-center text-gray-500 dark:text-gray-400 italic;
}

.inlineError {
  @apply mx-4 mt-3 mb-1 px-3 py-2 rounded-md text-sm;
  @apply bg-red-50 text-red-800 border border-red-200;
  @apply dark:bg-red-950/40 dark:text-red-200 dark:border-red-800;
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
  @apply flex items-center space-x-1;
}

.loading {
  @apply bg-gray-500 dark:bg-gray-600 cursor-wait;
}

.success {
  @apply bg-green-500 dark:bg-green-600 cursor-default;
}

.spinner {
  @apply inline-block w-4 h-4 mr-2;
  @apply border-2 border-white border-t-transparent rounded-full;
  @apply animate-spin;
}

.showButton {
  @apply bg-gray-500 hover:bg-gray-600 text-sm text-white py-1.5 px-3 rounded-md;
  @apply dark:bg-gray-600 dark:hover:bg-gray-700;
  @apply transition-colors duration-200;
}

.copyButton {
  @apply bg-slate-400 hover:bg-slate-500 text-sm text-white py-1.5 px-3 rounded-md;
  @apply dark:bg-slate-500 dark:hover:bg-slate-600;
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
  @apply flex-wrap;
}

.secretLabel {
  @apply font-medium text-slate-700 dark:text-slate-300 mr-2;
}

.secretValue {
  @apply font-mono text-gray-800 dark:text-gray-200;
  @apply bg-white dark:bg-gray-700 px-3 py-1.5 rounded-md;
  @apply border border-slate-300 dark:border-slate-600;
  @apply break-all overflow-auto max-w-full;
  max-height: 200px;
}

.addSecretButton {
  @apply bg-slate-100 hover:bg-slate-200 text-slate-700 font-medium py-2 px-4 rounded-md;
  @apply dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300;
  @apply border border-slate-300 dark:border-slate-600;
  @apply transition-colors duration-200;
}

.buttonGroup {
  @apply flex items-center space-x-6;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.animate-spin {
  animation: spin 1s linear infinite;
}
</style>
