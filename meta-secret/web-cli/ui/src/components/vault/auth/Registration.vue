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
    await this.checkVaultName();
    await this.checkIsLocal();
    await this.checkIsOutsider();
    await this.checkIsVaultNotExists();
  },

  methods: {
    async generate_user_creds() {
      await this.jsAppState.appManager.generate_user_creds(this.vaultName);
      await this.checkIsLocal();
    },

    async signUp() {
      console.log('Generate vault');
      await this.jsAppState.appManager.sign_up();
      await this.checkIsLocal();
    },

    async checkVaultName() {
      const currState = await this.jsAppState.appManager.get_state();
      if (currState.is_local()) {
        this.vaultName = '';
        return;
      }

      if (currState.is_vault()) {
        const vaultState = currState.as_vault();
        this.vaultName = vaultState.vault_name();
      } else {
        this.vaultName = '';
      }
    },

    async checkIsLocal() {
      const currState = await this.jsAppState.appManager.get_state();
      this.isLocalState = currState.is_local();
      console.log('is in Local state: ', this.isLocalState);
    },

    async checkIsOutsider() {
      const currState = await this.jsAppState.appManager.get_state();
      const isVault = currState.is_vault();

      if (!isVault) {
        this.isOutsiderState = false;
        return;
      }

      const vaultState = currState.as_vault();
      this.isOutsiderState = vaultState.is_outsider();
      console.log('is in Outsider state: ', this.isOutsiderState);
    },

    async checkIsVaultNotExists() {
      const currState = await this.jsAppState.appManager.get_state();
      const isVault = currState.is_vault();

      if (!isVault) {
        this.isOutsiderState = false;
        return;
      }

      const vaultState = currState.as_vault();
      this.isVaultNotExists = vaultState.is_vault_not_exists();
      console.log('is in VaultNotExists state: ', this.isVaultNotExists);
    },
  },
});
</script>

<template>

  <div v-cloak>
    <div v-if="isLocalState">
      <div class="container flex items-center max-w-md py-2">
        <label>Enter vault name:</label>
      </div>

      <div class="container flex items-center justify-center max-w-md border-b border-t border-l border-r py-2 px-2">
        <label>@</label>
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
  @apply appearance-none bg-transparent border-none w-full text-gray-700 mr-3 py-1 leading-tight focus:outline-none;
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
  @apply w-full text-gray-700 mr-3 py-1 px-2 leading-tight focus:outline-none;
}

/* v-cloak will hide components until Vue renders them */
[v-cloak] {
  display: none;
}
</style>
