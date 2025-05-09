<script setup lang="ts">
import { AppState } from '@/stores/app-state';
import { ref } from 'vue';
import LocalVaultCreation from './LocalVaultCreation.vue';
import ProgressSimulation from './ProgressSimulation.vue';

const jsAppState = AppState();
const vaultName = ref(jsAppState.getVaultName());
const signUpProcessing = ref(false);
const signUpCompleted = ref(false);

const signUp = async () => {
  if (signUpProcessing.value) {
    return;
  }

  signUpProcessing.value = true;
  signUpCompleted.value = false;

  try {
    // @ts-ignore - Method exists in Rust but TS definitions might be outdated
    await jsAppState.appManager.sign_up();

    // Mark the progress as completed
    signUpCompleted.value = true;

    // Small delay to allow the user to see 100% before reload
    setTimeout(() => {
      window.location.reload();
    }, 500);
  } catch (error) {
    signUpProcessing.value = false;
    signUpCompleted.value = false;
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

    <LocalVaultCreation v-if="jsAppState.isLocal" :signUpProcessing="signUpProcessing" />

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

    <ProgressSimulation :isActive="signUpProcessing" :completed="signUpCompleted" />
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
</style>
