<script lang="ts">
import { defineComponent } from 'vue';
import init, { MetaPasswordId, PlainPassInfo, WasmApplicationState } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';

export default defineComponent({
  async setup() {
    console.log('Secrets Component. Init');

    await init();
    const appState = AppState();

    return {
      newPassword: '',
      newPassDescription: '',
      appState: appState,
    };
  },

  methods: {
    async addPassword() {
      await init();
      const pass = new PlainPassInfo(this.newPassDescription, this.newPassword);
      await this.appState.appManager.cluster_distribution(pass);
    },

    async recover(metaPassId: MetaPasswordId) {
      await this.appState.appManager.recover_js(metaPassId);
    },

    metaPasswords(): MetaPasswordId[] {
      const msAppState: WasmApplicationState = this.appState.metaSecretAppState;
      return msAppState.as_vault().as_member().vault_data().secrets();
    },
  },
});
</script>

<template>
  <div class="py-2" />

  <div :class="$style.newPasswordDiv">
    <div class="flex items-center">
      <label>description: </label>
      <input type="text" :class="$style.passwordInput" placeholder="my meta secret" v-model="newPassDescription" />
    </div>
    <div class="flex items-center">
      <label>secret: </label>
      <input type="text" :class="$style.passwordInput" placeholder="top$ecret" v-model="newPassword" />
    </div>
    <div class="flex justify-end">
      <button :class="$style.addButton" @click="addPassword">Add</button>
    </div>
  </div>

  <div class="py-4" />

  <!-- https://www.tailwind-kit.com/components/list -->
  <div :class="$style.secrets">
    <ul class="w-full flex flex-col divide-y divide p-2">
      <li v-for="secret in this.metaPasswords()" :key="secret.id" class="flex flex-row">
        <div class="flex items-center flex-1 p-4 cursor-pointer select-none">
          <div class="flex-1 pl-1 mr-16">
            <div class="font-medium dark:text-white">
              {{ secret.name }}
            </div>
            <div class="text-sm text-gray-600 dark:text-gray-200">
              {{ secret.id() }}
            </div>
          </div>
          <button :class="$style.actionButtonText" @click="recover(secret)">Recover</button>
        </div>
      </li>
    </ul>
  </div>
</template>

<style module>
.secrets {
  @apply container max-w-md flex flex-col items-center justify-center w-full;
  @apply mx-auto bg-white shadow dark:bg-gray-800;
}

.newPasswordDiv {
  @apply block max-w-md mx-auto items-center justify-center border-b border-t border-l border-r py-2 px-4;
}

.passwordInput {
  @apply appearance-none bg-transparent border-none w-full text-gray-700 mr-3 py-1 px-2 leading-tight focus:outline-none;
}

.addButton {
  @apply flex-shrink-0 bg-orange-400 border-orange-500 text-sm border-2 text-white py-1 px-4 rounded;
  @apply hover:bg-orange-700 hover:border-orange-700;
}

.actionButtonText {
  @apply flex-shrink-0 bg-gray-700 border-gray-800 text-sm border-2 text-white py-1 px-4 rounded;
  @apply hover:bg-gray-900 hover:border-gray-900 transition-colors duration-200;
}
</style>
