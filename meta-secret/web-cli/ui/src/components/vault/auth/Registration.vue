<script lang="ts">
import { defineComponent } from 'vue';
import { AppState } from '@/stores/app-state';

export default defineComponent({
  data() {
    return {
      jsAppState: null,
      vaultName: '',
      isLocalState: false,
    };
  },

  async created() {
    console.log('JS: Registration component created');
    this.jsAppState = AppState();
  },

  async mounted() {
    console.log('Registration component mounted');
    await this.checkIsLocal();
  },

  methods: {
    async generate_user_creds() {
      await this.jsAppState.appManager.generate_user_creds(this.vaultName);
      await this.checkIsLocal();
    },

    async registration() {
      console.log('Generate vault');
      await this.jsAppState.appManager.sign_up(this.vaultName);
      await this.checkIsLocal();
    },

    async checkIsLocal() {
      console.log('Checking if state is local');
      const currState = await this.jsAppState.appManager.get_state();
      console.log('Current state:', currState);

      // Use original method without boolean conversion since it works
      const isLocal = currState.is_local();
      console.log('Is local state (converted):', isLocal);

      // Set state
      this.isLocalState = isLocal;
      console.log('Component isLocalState after setting:', this.isLocalState);
    },

    async isMember() {
      const currState = await this.jsAppState.appManager.get_state();
      const isVault = currState.is_vault();
      if (!isVault) {
        return false;
      }

      return this.metaSecretAppState.jsAppState.as_vault().is_member();
    },
  },
});
</script>

<template>
  <!-- Only render content when state check is complete -->
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
