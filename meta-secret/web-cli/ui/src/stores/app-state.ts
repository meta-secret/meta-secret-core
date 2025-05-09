import { defineStore } from 'pinia';
import init from 'meta-secret-web-cli';
import { ApplicationStateInfo, WasmApplicationManager } from 'meta-secret-web-cli';

export const AppState = defineStore('app_state', {
  state: () => {
    console.log('App state. Init');

    return {
      appManager: WasmApplicationManager,
      stateInfo: ApplicationStateInfo.Local,
    };
  },

  getters: {
    currentStateInfo: (state) => state.stateInfo,
    
    // Helper methods for state comparisons
    isLocal: (state) => state.stateInfo === ApplicationStateInfo.Local,
    isVaultNotExists: (state) => state.stateInfo === ApplicationStateInfo.VaultNotExists,
    isMember: (state) => state.stateInfo === ApplicationStateInfo.Member,
    isOutsider: (state) => state.stateInfo === ApplicationStateInfo.Outsider,
  },

  actions: {
    async appStateInit() {
      console.log('Js: App state, start initialization');
      
      await init();
      const appManager = await WasmApplicationManager.init_wasm();
      console.log('Js: Initial App State!!!!');

      this.appManager = appManager;
      
      // Update state info after initialization
      await this.updateStateInfo();

      // Temporary disabled: reactive app state!
      /*const subscribe = async (appManager: WasmApplicationManager) => {
        const state = await appManager.get_state();
        console.log('Js: Updated State: ', state);
        this.metaSecretAppState = state;

        await subscribe(appManager);
      };

      subscribe(appManager).then(() => console.log('Finished subscribing'));*/
    },

    async updateStateInfo() {
      const currState = await this.appManager.get_state();
      this.stateInfo = currState.as_info();
      return this.stateInfo;
    },

    async getVaultName() {
      const currState = await this.appManager.get_state();
      if (currState.as_info() == ApplicationStateInfo.Local) {
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
