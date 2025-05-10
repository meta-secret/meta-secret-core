<script setup>
import { AppState } from '@/stores/app-state';
import { ref } from 'vue';

const props = defineProps({
  signUpProcessing: Boolean
});

const jsAppState = AppState();
const vaultName = ref(jsAppState.getVaultName());

const updateVaultName = (event) => {
  const input = event.target;
  vaultName.value = input.value;
};

const generateUserCreds = async () => {
  if (props.signUpProcessing) {
    return;
  }
  // @ts-ignore - Method exists in Rust but TS definitions might be outdated
  await jsAppState.appManager.generate_user_creds(vaultName.value);
  window.location.reload();
};
</script>

<template>
  <div :class="$style.formContainer">
    <div :class="$style.labelContainer">
      <label :class="$style.formLabel">Enter vault name:</label>
    </div>

    <div :class="$style.inputWrapper">
      <div :class="$style.inputContainer">
        <span :class="$style.atSymbol">@</span>
        <input 
          :class="$style.vaultNameInput" 
          type="text" 
          placeholder="vault name" 
          :value="vaultName" 
          @input="updateVaultName"
          :disabled="signUpProcessing" 
        />
      </div>
      <button :class="$style.actionButton" @click="generateUserCreds" :disabled="signUpProcessing">Set Vault Name</button>
    </div>

    <div v-if="vaultName" :class="$style.vaultInfoMessage">
      <p>
        This will create a new vault named <span :class="$style.vaultNameHighlight">{{ vaultName }}</span>
      </p>
    </div>
  </div>
</template>

<style module>
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

.vaultNameInput {
  @apply appearance-none bg-transparent border-none w-full;
  @apply text-gray-800 dark:text-white mx-2 py-1 leading-tight focus:outline-none;
  @apply placeholder-gray-400;
}

.actionButton {
  @apply bg-orange-600 hover:bg-orange-700 text-white font-medium py-3 px-6 rounded-lg;
  @apply transition-colors duration-200 shadow-md;
  @apply text-sm md:text-base whitespace-nowrap;
}

.actionButton:disabled {
  @apply bg-gray-500 cursor-not-allowed;
}

.vaultNameHighlight {
  @apply font-bold text-orange-400;
}
</style> 