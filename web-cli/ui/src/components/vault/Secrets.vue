<script lang="ts">
import {defineComponent} from "vue";
import init from "meta-secret-web-cli";
import {AppState} from "@/stores/app-state";

export default defineComponent({
  async setup() {
    console.log("Secrets Component. Init");

    await init();
    const appState = AppState();

    return {
      newPassword: "",
      newPassDescription: "",
      appState: appState
    };
  },

  methods: {
    async addPassword() {
      await init();
      await this.appState
          .stateManager
          .cluster_distribution(this.newPassDescription, this.newPassword);
    },

    async recover(metaPassId) {
      await init();
      alert("Recover password: " + JSON.stringify(metaPassId));
      await this.appState.stateManager.recover_js(metaPassId);
    },
  },
});
</script>

<template>
  <div class="py-2" />

  <div :class="$style.newPasswordDiv">
    <div class="flex items-center">
      <label>description: </label>
      <input
        type="text"
        :class="$style.passwordInput"
        placeholder="my meta secret"
        v-model="newPassDescription"
      />
    </div>
    <div class="flex items-center">
      <label>secret: </label>
      <input
        type="text"
        :class="$style.passwordInput"
        placeholder="top$ecret"
        v-model="newPassword"
      />
    </div>
    <div class="flex justify-end">
      <button :class="$style.addButton" @click="addPassword">Add</button>
    </div>
  </div>

  <div class="py-4" />

  <!-- https://www.tailwind-kit.com/components/list -->
  <div :class="$style.secrets">
    <ul class="w-full flex flex-col divide-y divide p-2">
      <li
        v-for="secret in this.appState.internalState.metaPasswords"
        :key="secret.id.id"
        class="flex flex-row"
      >
        <div class="flex items-center flex-1 p-4 cursor-pointer select-none">
          <div class="flex-1 pl-1 mr-16">
            <div class="font-medium dark:text-white">
              {{ secret.id.name }}
            </div>
            <div class="text-sm text-gray-600 dark:text-gray-200">
              {{ secret.id.id.slice(0, 18) }}
            </div>
          </div>
          <button :class="$style.actionButtonText" @click="recover(secret.id)">
            Recover
          </button>
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
  @apply block max-w-md mx-auto items-center justify-center max-w-md border-b border-t border-l border-r py-2 px-4;
}

.passwordInput {
  @apply appearance-none bg-transparent border-none w-full text-gray-700 mr-3 py-1 px-2 leading-tight focus:outline-none;
}

.addButton {
  @apply flex-shrink-0 bg-orange-400 border-orange-500 text-sm border-2 text-white py-1 px-4 rounded;
  @apply hover:bg-orange-700 hover:border-orange-700;
}

.actionButtonText {
  @apply flex justify-end w-24 text-right;
}
</style>