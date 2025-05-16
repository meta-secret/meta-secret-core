import { defineStore } from 'pinia';
import init, { ApplicationStateInfo, WasmApplicationManager, WasmApplicationState } from 'meta-secret-web-cli';

export const AppState = defineStore('app_state', {
  state: () => {
    console.log('App state. Init');

    return {
      appManager: WasmApplicationManager,
      currState: WasmApplicationState,
    };
  },

  getters: {
    currentState: (state) => state.currState,
    passwords: (state) => {
      return state.currState.as_vault().as_member().vault_data().secrets();
    },

    // Helper methods for state comparisons
    isLocal: (state) => {
      return state.currState.as_info() === ApplicationStateInfo.Local;
    },
    isVaultNotExists: (state) => {
      return state.currState.as_info() === ApplicationStateInfo.VaultNotExists;
    },
    isMember: (state) => {
      return state.currState.as_info() === ApplicationStateInfo.Member;
    },
    isOutsider: (state) => {
      return state.currState.as_info() === ApplicationStateInfo.Outsider;
    },
  },

  actions: {
    async appStateInit() {
      console.log('Js: App state, start initialization');

      await init();
      const appManager = await WasmApplicationManager.init_wasm();
      console.log('Js: Initial App State!!!!');

      this.appManager = appManager;
      await this.updateState();

      // Temporary disabled: reactive app state!
      /*const subscribe = async (appManager: WasmApplicationManager) => {
        const state = await appManager.get_state();
        console.log('Js: Updated State: ', state);
        this.metaSecretAppState = state;

        await subscribe(appManager);
      };

      subscribe(appManager).then(() => console.log('Finished subscribing'));*/
    },

    async updateState() {
      this.currState = await this.appManager.get_state();
      return this.currState;
    },

    updateStateWith(newState: WasmApplicationState) {
      this.currState = newState;
      return this.currState;
    },

    getVaultName() {
      const currState = this.currState;
      if (!currState) return '';

      const currStateInfo = currState.as_info();
      if (currStateInfo === ApplicationStateInfo.Local) {
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
