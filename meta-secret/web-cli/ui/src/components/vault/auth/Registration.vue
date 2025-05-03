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
  <div v-cloak>
    <div
      v-if="!isLocalState && vaultName"
      class="container max-w-md py-2 px-4 mb-3 bg-gray-100 dark:bg-gray-700 rounded-md border border-gray-200 dark:border-gray-600"
    >
      <div class="flex items-center justify-between">
        <div class="text-gray-700 dark:text-gray-200">
          <span class="text-sm font-medium">Vault Name:</span>
          <span class="ml-1 text-md font-bold text-teal-600 dark:text-teal-400">{{ vaultName }}</span>
        </div>
      </div>
    </div>

    <div v-if="isLocalState">
      <div class="container flex items-center max-w-md py-2">
        <label class="text-gray-700 dark:text-gray-300">Enter vault name:</label>
      </div>

      <div class="container flex items-center justify-center max-w-md border-b border-t border-l border-r py-2 px-2 dark:border-gray-700 dark:bg-gray-800">
        <label class="text-gray-700 dark:text-gray-300">@</label>
        <input :class="$style.nicknameUserInput" type="text" placeholder="vault name" v-model="vaultName" />

        <button :class="$style.registrationButton" @click="generate_user_creds">Create User Creds</button>
      </div>
    </div>

    <div v-if="isOutsiderState">
      <div class="container flex items-center max-w-md py-2">
        <label :class="$style.joinLabel">Vault already exists, would you like to join?</label>
        <button :class="$style.joinButton" @click="signUp">Join</button>
      </div>
    </div>

    <div v-if="isVaultNotExists">
      <div class="container flex items-center max-w-md py-2">
        <label :class="$style.joinLabel">Vault doesn't exist, let's create one!</label>
        <button :class="$style.joinButton" @click="signUp">Create</button>
      </div>
    </div>
  </div>
</template>

<style module>
.joinLabel {
  @apply appearance-none bg-transparent border-none w-full text-gray-700 dark:text-gray-300 mr-3 py-1 leading-tight focus:outline-none;
}

.registrationButton {
  @apply flex-shrink-0 bg-teal-500 border-teal-500 text-sm border-4 text-white py-1 px-4 rounded;
  @apply hover:bg-teal-700 hover:border-teal-700;
}

.joinButton {
  @apply flex-shrink-0 bg-teal-500;
  @apply hover:bg-teal-700 border-teal-500 hover:border-teal-700 text-sm border-4 text-white py-1 px-4 rounded;
}

.nicknameUserInput {
  @apply appearance-none bg-transparent border-none;
  @apply w-full text-gray-700 dark:text-gray-200 mr-3 py-1 px-2 leading-tight focus:outline-none;
}

/* v-cloak will hide components until Vue renders them */
[v-cloak] {
  display: none;
}
</style>
