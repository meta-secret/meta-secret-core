import { defineStore } from 'pinia';
import init, {
  ApplicationStateInfo,
  MasterKeyManager,
  WasmApplicationManager,
  WasmApplicationState,
} from 'meta-secret-web-cli';
import { useAuthStore } from '@/stores/auth';

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
    resolveWebDeviceInfo() {
      const ua = navigator.userAgent.toLowerCase();
      const platform = navigator.platform || 'Web';
      const browser =
        ua.includes('edg') ? 'Edge' :
        ua.includes('opr') || ua.includes('opera') ? 'Opera' :
        ua.includes('firefox') ? 'Firefox' :
        ua.includes('safari') && !ua.includes('chrome') ? 'Safari' :
        ua.includes('chrome') ? 'Chrome' :
        'Browser';
      const isTablet = /ipad|tablet|android(?!.*mobile)/i.test(navigator.userAgent);
      const deviceType = isTablet ? 'Tablet' : 'Web';
      const deviceName = `${browser} on ${platform}`.trim();
      return { deviceName, deviceType };
    },

    async appStateInit() {
      console.log('Js: App state, start initialization');
      await init();

      const authStore = useAuthStore();
      const transportSk = MasterKeyManager.from_pure_sk(authStore.masterKey);
      const { deviceName, deviceType } = this.resolveWebDeviceInfo();
      const appManager = await WasmApplicationManager.init_wasm_with_device(
        transportSk,
        deviceName,
        deviceType,
      );
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
