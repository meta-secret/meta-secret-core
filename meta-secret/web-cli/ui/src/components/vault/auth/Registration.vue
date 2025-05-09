<script setup lang="ts">
import { AppState } from '@/stores/app-state';
import { ref } from 'vue';

const jsAppState = AppState();
const vaultName = ref(jsAppState.getVaultName());
const signUpProcessing = ref(false);
const progress = ref(0);
const progressInterval = ref(null as NodeJS.Timeout | null);

const generate_user_creds = async () => {
  if (signUpProcessing.value) return;
  await jsAppState.appManager.generate_user_creds(vaultName.value);
  window.location.reload();
};

const startProgressSimulation = () => {
  // Simulate progress with intervals (since we don't have actual progress feedback)
  progress.value = 0;
  progressInterval.value = setInterval(() => {
    // Never reach 100% until actual completion
    if (progress.value < 90) {
      progress.value += Math.random() * 10;
    }
  }, 200);
};

const signUp = async () => {
  if (signUpProcessing.value) {
    return;
  }
  signUpProcessing.value = true;
  startProgressSimulation();
  
  try {
    // @ts-ignore - Method exists in Rust but TS definitions might be outdated
    await jsAppState.appManager.sign_up();
    if (progressInterval.value) clearInterval(progressInterval.value);
    progress.value = 100;
    window.location.reload();
  } catch (error) {
    signUpProcessing.value = false;
    if (progressInterval.value) {
      clearInterval(progressInterval.value);
    }
    progress.value = 0;
    // In a real app, you'd want to handle the error here
  }
};
</script>

<template>
  <div :class="$style.container">
    <div :class="$style.header">
      <p v-if="jsAppState.isVaultNotExists" :class="$style.titleText">Creating new vault</p>
      <p v-else-if="jsAppState.isOutsider" :class="$style.titleText">
        Joining existing vault: <span :class="$style.vaultNameHighlight">{{ vaultName }}</span>
      </p>
    </div>

    <div v-if="!jsAppState.isLocal" :class="$style.vaultInfoContainer">
      <div :class="$style.vaultInfoRow">
        <div :class="$style.vaultInfoText">
          <span :class="$style.vaultInfoLabel">Vault Name:</span>
          <span :class="$style.vaultInfoValue">{{ vaultName }}</span>
        </div>
      </div>
    </div>

    <div v-if="jsAppState.isLocal" :class="$style.formContainer">
      <div :class="$style.labelContainer">
        <label :class="$style.formLabel">Enter vault name:</label>
      </div>

      <div :class="$style.inputWrapper">
        <div :class="$style.inputContainer">
          <span :class="$style.atSymbol">@</span>
          <input :class="$style.vaultNameInput" type="text" placeholder="vault name" v-model="vaultName" :disabled="signUpProcessing" />
        </div>
        <button :class="$style.actionButton" @click="generate_user_creds" :disabled="signUpProcessing">Set Vault Name</button>
      </div>

      <div v-if="vaultName" :class="$style.vaultInfoMessage">
        <p>
          This will create a new vault named <span :class="$style.vaultNameHighlight">{{ vaultName }}</span>
        </p>
      </div>
    </div>

    <div v-if="jsAppState.isOutsider" :class="$style.optionContainer">
      <div :class="$style.statusContainer">
        <label :class="$style.statusLabel">Vault already exists, would you like to join?</label>
        <button :class="$style.actionButton" @click="signUp" :disabled="signUpProcessing">Join</button>
      </div>
    </div>

    <div v-if="jsAppState.isVaultNotExists" :class="$style.optionContainer">
      <div :class="$style.statusContainer">
        <label :class="$style.statusLabel">Vault doesn't exist, let's create one!</label>
        <button :class="$style.actionButton" @click="signUp" :disabled="signUpProcessing">Create</button>
      </div>
    </div>

    <div v-if="signUpProcessing" :class="$style.progressContainer">
      <div :class="$style.warningBox">
        <div :class="$style.warningHeader">
          <span :class="$style.warningIcon">⚠️</span>
          <span :class="$style.warningTitle">Creating Vault...</span>
        </div>
        <p :class="$style.warningMessage">Please don't close this page. Vault creation is in progress...</p>
        
        <div :class="$style.progressBarContainer">
          <div :class="$style.progressBar" :style="{ width: `${Math.min(progress, 100)}%` }"></div>
        </div>
        
        <p :class="$style.progressText">{{ Math.floor(progress) }}%</p>
      </div>
    </div>
  </div>
</template>

<style module>
.container {
  @apply flex flex-col items-center justify-center;
}

.header {
  @apply text-center mb-6 mt-4;
}

.titleText {
  @apply text-xl text-gray-300;
}

.vaultNameHighlight {
  @apply font-bold text-orange-400;
}

.vaultInfoContainer {
  @apply container max-w-md py-3 px-5 mb-4 rounded-lg;
  @apply bg-gray-800 border border-gray-700;
  @apply shadow-lg transition-all duration-200;
}

.vaultInfoRow {
  @apply flex items-center justify-between;
}

.vaultInfoText {
  @apply text-gray-300;
}

.vaultInfoLabel {
  @apply text-sm font-medium;
}

.vaultInfoValue {
  @apply ml-1 text-base font-bold text-orange-400;
}

.formContainer {
  @apply w-full max-w-md;
}

.labelContainer {
  @apply mb-2 text-left;
}

.formLabel {
  @apply text-white text-xl mb-2;
}

.inputWrapper {
  @apply flex flex-col md:flex-row gap-4 items-center justify-between;
  @apply w-full;
}

.inputContainer {
  @apply flex items-center w-full;
  @apply bg-white dark:bg-gray-700 rounded-lg px-4 py-3;
  @apply border border-gray-300 dark:border-gray-600;
  @apply transition-all duration-200 shadow-lg;
}

.atSymbol {
  @apply text-gray-400 text-xl;
}

.vaultInfoMessage {
  @apply mt-4 text-center text-gray-400;
}

.optionContainer {
  @apply w-full max-w-md mt-6;
}

.statusContainer {
  @apply flex items-center justify-between py-4 px-5 rounded-lg;
  @apply bg-gray-800 border border-gray-700;
  @apply shadow-lg transition-all duration-200;
}

.statusLabel {
  @apply text-gray-300 text-sm md:text-base;
}

.actionButton {
  @apply bg-orange-600 hover:bg-orange-700 text-white font-medium py-3 px-6 rounded-lg;
  @apply transition-colors duration-200 shadow-md;
  @apply text-sm md:text-base whitespace-nowrap;
}

.actionButton:disabled {
  @apply bg-gray-500 cursor-not-allowed;
}

.vaultNameInput {
  @apply appearance-none bg-transparent border-none w-full;
  @apply text-gray-800 dark:text-white mx-2 py-1 leading-tight focus:outline-none;
  @apply placeholder-gray-400;
}

.progressContainer {
  @apply w-full max-w-md mt-8;
}

.warningBox {
  @apply bg-gray-800 border border-yellow-600 rounded-lg p-4;
  @apply shadow-lg transition-all duration-200;
}

.warningHeader {
  @apply flex items-center mb-2;
}

.warningIcon {
  @apply text-xl mr-2;
}

.warningTitle {
  @apply text-yellow-500 font-bold text-lg;
}

.warningMessage {
  @apply text-gray-300 mb-4 text-sm;
}

.progressBarContainer {
  @apply w-full h-2 bg-gray-700 rounded-full mb-2;
  @apply overflow-hidden;
}

.progressBar {
  @apply h-full bg-yellow-500 rounded-full;
  @apply transition-all duration-200;
}

.progressText {
  @apply text-center text-gray-400 text-sm;
}
</style>

