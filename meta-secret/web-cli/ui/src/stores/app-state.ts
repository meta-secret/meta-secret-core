import { defineStore } from 'pinia';
import { WasmApplicationManager, WasmApplicationState } from 'meta-secret-web-cli';

export const AppState = defineStore('app_state', {
  state: () => {
    console.log('App state. Init');

    return {
      appManager: WasmApplicationManager,
      metaSecretAppState: WasmApplicationState,
    };
  },

  actions: {
    async appStateInit() {
      console.log('Js: App state, start initialization');

      const appManager = await WasmApplicationManager.init_wasm();
      this.metaSecretAppState = await appManager.get_state();
      console.log('Js: Initial App State!!!!');

      this.appManager = appManager;

      // Temporary disabled: reactive app state!
      /*const subscribe = async (appManager: WasmApplicationManager) => {
        const state = await appManager.get_state();
        console.log('Js: Updated State: ', state);
        this.metaSecretAppState = state;

        await subscribe(appManager);
      };

      subscribe(appManager).then(() => console.log('Finished subscribing'));*/
    },

    async stateInfo() {
      const currState = await this.appManager.get_state();
      return currState.as_info();
    },

    async getVaultName() {
      const currState = await this.appManager.get_state();
      if (currState.is_local()) {
        return '';
      }

      if (currState.is_vault()) {
        const vaultState = currState.as_vault();
        return vaultState.vault_name();
      }

      return '';
    },
  },
});
