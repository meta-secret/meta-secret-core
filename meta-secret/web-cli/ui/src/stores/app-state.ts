import { defineStore } from 'pinia';
import init, {
  ApplicationStateInfo,
  MasterKeyManager,
  WasmApplicationManager,
  WasmApplicationState,
} from 'meta-secret-web-cli';
import { useAuthStore } from '@/stores/auth';
import { META_SECRET_ENVIRONMENT } from '@/config/metaSecretEnvironment';
import { createAppStateLane, type AppStateLaneStore } from './appStateLane';

const lanes = new WeakMap<AppStateLaneStore, ReturnType<typeof createAppStateLane>>();
function stateLane(store: AppStateLaneStore) {
  let lane = lanes.get(store);
  if (!lane) {
    lane = createAppStateLane({
      store,
      identity: () => {
        const auth = useAuthStore();
        return { key: auth.masterKey, authenticated: auth.isAuthenticated };
      },
    });
    lanes.set(store, lane);
  }
  return lane;
}

export const AppState = defineStore('app_state', {
  state: () => {
    return {
      appManager: null as WasmApplicationManager | null,
      currState: null as WasmApplicationState | null,
      pendingSyncCount: 0,
      syncError: '',
      sessionGeneration: 0,
      requiredSyncPending: false,
    };
  },

  getters: {
    isSynchronizing: (state) => state.pendingSyncCount > 0,
    currentState: (state) => state.currState,
    passwords: (state) => {
      if (!state.currState) return [];
      return state.currState.as_vault().as_member().vault_data().secrets();
    },

    // Helper methods for state comparisons
    isLocal: (state) => {
      if (!state.currState) return false;
      return state.currState.as_info() === ApplicationStateInfo.Local;
    },
    isVaultNotExists: (state) => {
      if (!state.currState) return false;
      return state.currState.as_info() === ApplicationStateInfo.VaultNotExists;
    },
    isMember: (state) => {
      if (!state.currState) return false;
      return state.currState.as_info() === ApplicationStateInfo.Member;
    },
    isOutsider: (state) => {
      if (!state.currState) return false;
      return state.currState.as_info() === ApplicationStateInfo.Outsider;
    },
  },

  actions: {
    beginSync() {
      this.pendingSyncCount += 1;
    },
    endSync() {
      this.pendingSyncCount = Math.max(0, this.pendingSyncCount - 1);
    },
    async clearIndexedDbByName(dbName: string) {
      await new Promise<void>((resolve, reject) => {
        const request = indexedDB.deleteDatabase(dbName);

        request.onsuccess = () => resolve();
        request.onerror = () => reject(request.error);
        request.onblocked = () => resolve();
      });
    },

    async clearAllMetaSecretIndexedDb() {
      const dbNames = ['meta-secret', 'meta-secret-server', 'meta-secret-v-device', 'meta-secret-v-device-2'];

      for (const dbName of dbNames) {
        try {
          await this.clearIndexedDbByName(dbName);
        } catch (error) {
          console.warn(`Failed to delete IndexedDB ${dbName}:`, error);
        }
      }
    },

    shouldResetDbOnInitError(error: unknown) {
      const text = String(error);
      return text.includes('missing field `deviceType`') || text.includes('missing field deviceType');
    },

    async cleanDatabase() {
      return stateLane(this).clean(async (manager) => {
        if (manager && typeof manager.clean_up_database === 'function') await manager.clean_up_database();
        else await this.clearAllMetaSecretIndexedDb();
      });
    },

    resolveWebDeviceInfo() {
      const ua = navigator.userAgent.toLowerCase();
      const platform = navigator.platform || 'Web';
      const browser = ua.includes('edg')
        ? 'Edge'
        : ua.includes('opr') || ua.includes('opera')
          ? 'Opera'
          : ua.includes('firefox')
            ? 'Firefox'
            : ua.includes('safari') && !ua.includes('chrome')
              ? 'Safari'
              : ua.includes('chrome')
                ? 'Chrome'
                : 'Browser';
      const isTablet = /ipad|tablet|android(?!.*mobile)/i.test(navigator.userAgent);
      const deviceType = isTablet ? 'Tablet' : 'Web';
      const deviceName = `${browser} on ${platform}`.trim();
      return { deviceName, deviceType };
    },

    async appStateInit() {
      return stateLane(this).initialize(async (key, current) => {
        await init();
        if (!current()) throw new Error('OperationUnavailable');
        const { deviceName, deviceType } = this.resolveWebDeviceInfo();
        const create = () =>
          WasmApplicationManager.init_wasm_with_device_and_environment(
            MasterKeyManager.from_pure_sk(key),
            deviceName,
            deviceType,
            META_SECRET_ENVIRONMENT,
          );
        try {
          return await create();
        } catch (error) {
          if (!current() || !this.shouldResetDbOnInitError(error)) throw new Error('OperationUnavailable');
          await this.clearAllMetaSecretIndexedDb();
          if (!current()) throw new Error('OperationUnavailable');
          return create();
        }
      });
    },

    async updateState() {
      return stateLane(this).refresh({ coalesce: false });
    },
    refreshState(options: { requiredSync?: boolean } = {}) {
      return stateLane(this).refresh(options);
    },
    invalidateSession() {
      stateLane(this).invalidate();
    },

    async getStateEventsAuthToken(vaultName = this.getVaultName()) {
      return stateLane(this).authorize((manager) => {
        if (typeof manager.state_events_auth_token !== 'function') throw new Error('OperationUnavailable');
        return manager.state_events_auth_token(vaultName);
      });
    },

    async getJoinResultAuthToken() {
      return stateLane(this).authorize((manager) => {
        if (typeof manager.join_result_auth_token !== 'function') throw new Error('OperationUnavailable');
        return manager.join_result_auth_token();
      });
    },

    updateStateWith(newState: WasmApplicationState) {
      return stateLane(this).publish(newState);
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
