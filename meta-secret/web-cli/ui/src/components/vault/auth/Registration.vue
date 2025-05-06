<script lang="ts">
import { defineComponent } from 'vue';
import { AppState } from '@/stores/app-state';

export default defineComponent({
  emits: ['state-changed'],
  
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
    this.jsAppState = AppState();
  },

  async mounted() {
    console.log("Mounted!")

    this.vaultName = await this.jsAppState.getVaultName();
    this.isLocalState = await this.jsAppState.checkIsLocal();
    this.isOutsiderState = await this.jsAppState.checkIsOutsider();
    this.isVaultNotExists = await this.jsAppState.checkIsVaultNotExists();
  },

  methods: {
    async generate_user_creds() {
      await this.jsAppState.appManager.generate_user_creds(this.vaultName);
      
      // Update local component state
      this.isLocalState = await this.jsAppState.checkIsLocal();
      this.isOutsiderState = await this.jsAppState.checkIsOutsider();
      this.isVaultNotExists = await this.jsAppState.checkIsVaultNotExists();
      this.vaultName = await this.jsAppState.getVaultName();
      
      console.log("User creds generated, emitting state change event");
      
      // Emit event to parent component
      this.$emit('state-changed');
    },

    async signUp() {
      console.log('Generate vault');
      await this.jsAppState.appManager.sign_up();
      
      // Update local component state
      this.isLocalState = await this.jsAppState.checkIsLocal();
      
      console.log("Signup complete, emitting state change event");
      
      // Emit event to parent component
      this.$emit('state-changed');
    },
  },
});
</script>

<template>
  <div v-cloak :class="$style.container">
    <div :class="$style.header">
      <p v-if="isVaultNotExists && vaultName" :class="$style.titleText">
        Creating new vault: <span :class="$style.vaultNameHighlight">{{ vaultName }}</span>
      </p>
      <p v-else-if="isOutsiderState && vaultName" :class="$style.titleText">
        Joining existing vault: <span :class="$style.vaultNameHighlight">{{ vaultName }}</span>
      </p>
    </div>

    <div v-if="!isLocalState && vaultName" :class="$style.vaultInfoContainer">
      <div :class="$style.vaultInfoRow">
        <div :class="$style.vaultInfoText">
          <span :class="$style.vaultInfoLabel">Vault Name:</span>
          <span :class="$style.vaultInfoValue">{{ vaultName }}</span>
        </div>
      </div>
    </div>

    <div v-if="isLocalState" :class="$style.formContainer">
      <div :class="$style.labelContainer">
        <label :class="$style.formLabel">Enter vault name:</label>
      </div>

      <div :class="$style.inputWrapper">
        <div :class="$style.inputContainer">
          <span :class="$style.atSymbol">@</span>
          <input :class="$style.vaultNameInput" type="text" placeholder="vault name" v-model="vaultName" />
        </div>
        <button :class="$style.actionButton" @click="generate_user_creds">Set Vault Name</button>
      </div>

      <div v-if="vaultName" :class="$style.vaultInfoMessage">
        <p>
          This will create a new vault named <span :class="$style.vaultNameHighlight">{{ vaultName }}</span>
        </p>
      </div>
    </div>

    <div v-if="isOutsiderState" :class="$style.optionContainer">
      <div :class="$style.statusContainer">
        <label :class="$style.statusLabel">Vault already exists, would you like to join?</label>
        <button :class="$style.actionButton" @click="signUp">Join</button>
      </div>
    </div>

    <div v-if="isVaultNotExists" :class="$style.optionContainer">
      <div :class="$style.statusContainer">
        <label :class="$style.statusLabel">Vault doesn't exist, let's create one!</label>
        <button :class="$style.actionButton" @click="signUp">Create</button>
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
