import { defineStore } from 'pinia';
import init, {
  ApplicationStateInfo,
  MasterKeyManager,
  WasmApplicationManager,
  WasmApplicationState,
} from 'meta-secret-web-cli';
import { useAuthStore } from '@/stores/auth';

let metaWsOnlineListenerRegistered = false;

/** Ensures a single WASM init + background client even if multiple views call appStateInit(). */
let appStateInitSingleton: Promise<void> | null = null;

const MAX_META_WS_BACKOFF_MS = 30_000;
/** Coalesce rapid WebSocket pushes into one UI/state refresh (avoids GetState spam). */
const META_WS_UPDATE_STATE_DEBOUNCE_MS = 250;

/** Same-origin `/meta_ws` is only valid when the UI and API share a host; static GitHub Pages must use the real API host. */
function metaWsUrlFromHttpApiBase(apiBase: string): string {
  const trimmed = apiBase.trim().replace(/\/$/, '');
  if (trimmed.startsWith('https://')) {
    return `${trimmed.replace(/^https:\/\//, 'wss://')}/meta_ws`;
  }
  if (trimmed.startsWith('http://')) {
    return `${trimmed.replace(/^http:\/\//, 'ws://')}/meta_ws`;
  }
  return `ws://${trimmed.replace(/^\//, '')}/meta_ws`;
}

function resolveMetaWsUrl(): string {
  const explicit = import.meta.env.VITE_META_WS_URL;
  if (typeof explicit === 'string' && explicit.length > 0) {
    return explicit;
  }
  const apiBase = import.meta.env.VITE_API_HTTP_BASE;
  if (typeof apiBase === 'string' && apiBase.length > 0) {
    return metaWsUrlFromHttpApiBase(apiBase);
  }
  if (typeof window !== 'undefined' && window.location.hostname === 'meta-secret.github.io') {
    return 'wss://api.meta-secret.org:443/meta_ws';
  }
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${protocol}//${window.location.host}/meta_ws`;
}

export const AppState = defineStore('app_state', {
  state: () => {
    console.log('App state. Init');

    return {
      appManager: WasmApplicationManager,
      currState: WasmApplicationState,
      metaWs: null as WebSocket | null,
      metaWsReconnectTimer: null as ReturnType<typeof setTimeout> | null,
      metaWsUpdateStateTimer: null as ReturnType<typeof setTimeout> | null,
      metaWsBackoffMs: 1000,
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
      if (appStateInitSingleton !== null) {
        return appStateInitSingleton;
      }
      const run = (async () => {
        console.log('Js: App state, start initialization');
        await init();

        const authStore = useAuthStore();
        const transportSk = MasterKeyManager.from_pure_sk(authStore.masterKey);
        const appManager = await WasmApplicationManager.init_wasm(transportSk);
        console.log('Js: Initial App State!!!!');

        this.appManager = appManager;
        await this.updateState();
        this.registerMetaWsOnlineListener();
        this.attachMetaWs();

        // Temporary disabled: reactive app state!
        /*const subscribe = async (appManager: WasmApplicationManager) => {
        const state = await appManager.get_state();
        console.log('Js: Updated State: ', state);
        this.metaSecretAppState = state;

        await subscribe(appManager);
      };

      subscribe(appManager).then(() => console.log('Finished subscribing'));*/
      })();
      appStateInitSingleton = run.catch((err) => {
        appStateInitSingleton = null;
        throw err;
      });
      return appStateInitSingleton;
    },

    clearMetaWsReconnectTimer() {
      if (this.metaWsReconnectTimer != null) {
        clearTimeout(this.metaWsReconnectTimer);
        this.metaWsReconnectTimer = null;
      }
    },

    clearMetaWsUpdateStateDebounce() {
      if (this.metaWsUpdateStateTimer != null) {
        clearTimeout(this.metaWsUpdateStateTimer);
        this.metaWsUpdateStateTimer = null;
      }
    },

    scheduleMetaWsUpdateState() {
      this.clearMetaWsUpdateStateDebounce();
      this.metaWsUpdateStateTimer = setTimeout(() => {
        this.metaWsUpdateStateTimer = null;
        void this.updateState();
      }, META_WS_UPDATE_STATE_DEBOUNCE_MS);
    },

    registerMetaWsOnlineListener() {
      if (metaWsOnlineListenerRegistered || typeof window === 'undefined') {
        return;
      }
      metaWsOnlineListenerRegistered = true;
      window.addEventListener('online', () => {
        this.metaWsBackoffMs = 1000;
        this.clearMetaWsReconnectTimer();
        if (this.metaWs) {
          this.metaWs.close();
        }
      });
    },

    attachMetaWs() {
      this.clearMetaWsReconnectTimer();
      this.clearMetaWsUpdateStateDebounce();
      this.metaWsBackoffMs = 1000;
      this.openMetaWsSocket();
    },

    openMetaWsSocket() {
      if (this.metaWs) {
        return;
      }
      const wsUrl = resolveMetaWsUrl();
      const ws = new WebSocket(wsUrl);
      const scheduleReconnect = () => {
        this.metaWs = null;
        const delay = Math.min(this.metaWsBackoffMs, MAX_META_WS_BACKOFF_MS);
        this.metaWsBackoffMs = Math.min(this.metaWsBackoffMs * 2, MAX_META_WS_BACKOFF_MS);
        this.clearMetaWsReconnectTimer();
        this.metaWsReconnectTimer = setTimeout(() => {
          this.metaWsReconnectTimer = null;
          this.openMetaWsSocket();
        }, delay);
      };
      ws.onopen = () => {
        this.metaWsBackoffMs = 1000;
      };
      ws.onmessage = async (ev: MessageEvent) => {
        if (typeof ev.data !== 'string') {
          return;
        }
        await this.appManager.apply_meta_ws_payload(ev.data);
        this.scheduleMetaWsUpdateState();
      };
      ws.onerror = () => {
        console.warn('meta_ws: connection error');
        ws.close();
      };
      ws.onclose = () => {
        scheduleReconnect();
      };
      this.metaWs = ws;
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
