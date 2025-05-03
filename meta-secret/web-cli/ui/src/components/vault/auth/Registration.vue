<script lang="ts">
import { defineComponent } from 'vue';
import { AppState } from '@/stores/app-state';

export default defineComponent({
  data() {
    return {
      jsAppState: null,
      vaultName: '',
      isLocalState: false,
      isOutsiderState: false,
      isVaultNotExists: false,
    };
  },

  async created() {
    console.log('JS: Registration component created');
    this.jsAppState = AppState();
  },

  async mounted() {
    console.log('Registration component mounted');
    this.vaultName = await this.jsAppState.getVaultName();
    this.isLocalState = await this.jsAppState.checkIsLocal();
    this.isOutsiderState = await this.jsAppState.checkIsOutsider();
    this.isVaultNotExists = await this.jsAppState.checkIsVaultNotExists();

    console.log('is in Local state: ', this.isLocalState);
    console.log('is in Outsider state: ', this.isOutsiderState);
    console.log('is in VaultNotExists state: ', this.isVaultNotExists);
  },

  methods: {
    async generate_user_creds() {
      await this.jsAppState.appManager.generate_user_creds(this.vaultName);
      this.isLocalState = await this.jsAppState.checkIsLocal();
    },

    async signUp() {
      console.log('Generate vault');
      await this.jsAppState.appManager.sign_up();
      this.isLocalState = await this.jsAppState.checkIsLocal();
    },
  },
});
</script>

<template>
  <div v-cloak class="flex flex-col items-center justify-center">
    <div class="text-center mb-6 mt-4">
      <p v-if="isVaultNotExists && vaultName" class="text-xl text-gray-300">
        Creating new vault: <span class="font-bold text-orange-400">{{ vaultName }}</span>
      </p>
      <p v-else-if="isOutsiderState && vaultName" class="text-xl text-gray-300">
        Joining existing vault: <span class="font-bold text-orange-400">{{ vaultName }}</span>
      </p>
    </div>

    <div v-if="!isLocalState && vaultName" :class="$style.vaultInfoContainer">
      <div class="flex items-center justify-between">
        <div class="text-gray-300">
          <span class="text-sm font-medium">Vault Name:</span>
          <span class="ml-1 text-md font-bold text-orange-400">{{ vaultName }}</span>
        </div>
      </div>
    </div>

    <div v-if="isLocalState" class="w-full max-w-md">
      <div :class="$style.labelContainer">
        <label class="text-white text-xl mb-2">Enter vault name:</label>
      </div>

      <div :class="$style.inputWrapper">
        <div :class="$style.inputContainer">
          <span class="text-gray-400 text-xl">@</span>
          <input :class="$style.vaultNameInput" type="text" placeholder="vault name" v-model="vaultName" />
        </div>
        <button :class="$style.actionButton" @click="generate_user_creds">Create User Creds</button>
      </div>
      
      <div v-if="vaultName" class="mt-4 text-center text-gray-400">
        <p>This will create a new vault named <span class="font-bold text-orange-400">{{ vaultName }}</span></p>
      </div>
    </div>

    <div v-if="isOutsiderState" class="w-full max-w-md mt-6">
      <div :class="$style.statusContainer">
        <label :class="$style.statusLabel">Vault already exists, would you like to join?</label>
        <button :class="$style.actionButton" @click="signUp">Join</button>
      </div>
    </div>

    <div v-if="isVaultNotExists" class="w-full max-w-md mt-6">
      <div :class="$style.statusContainer">
        <label :class="$style.statusLabel">Vault doesn't exist, let's create one!</label>
        <button :class="$style.actionButton" @click="signUp">Create</button>
      </div>
    </div>
  </div>
</template>

<style module>
.vaultInfoContainer {
  @apply container max-w-md py-3 px-5 mb-4 rounded-lg;
  @apply bg-gray-800 border border-gray-700;
  @apply shadow-lg transition-all duration-200;
}

.labelContainer {
  @apply mb-2 text-left;
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

.vaultNameInput {
  @apply appearance-none bg-transparent border-none w-full;
  @apply text-gray-800 dark:text-white mx-2 py-1 leading-tight focus:outline-none;
  @apply placeholder-gray-400;
}

/* v-cloak will hide components until Vue renders them */
[v-cloak] {
  display: none;
}
</style>
