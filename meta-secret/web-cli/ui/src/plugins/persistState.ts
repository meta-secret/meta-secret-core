import { PiniaPluginContext } from 'pinia';
import { watch } from 'vue';

interface PersistOptions {
  key?: string;
}

/**
 * Plugin to persist Pinia store state
 * This replaces localStorage direct usage with a more Vue-friendly approach
 */
export function createPersistedState(options: PersistOptions = {}) {
  return ({ store }: PiniaPluginContext) => {
    const storeKey = options.key || `pinia-${store.$id}`;
    
    // Initialize store with persisted state if available
    const persistedState = localStorage.getItem(storeKey);
    if (persistedState) {
      try {
        store.$patch(JSON.parse(persistedState));
      } catch (error) {
        console.error('Failed to parse stored state', error);
      }
    }
    
    // Watch for store changes and persist them
    watch(
      () => store.$state,
      (state) => {
        localStorage.setItem(storeKey, JSON.stringify(state));
      },
      { deep: true }
    );
  };
}

export default createPersistedState; 