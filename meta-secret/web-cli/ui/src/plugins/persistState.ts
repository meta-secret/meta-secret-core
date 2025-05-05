import { PiniaPluginContext } from 'pinia';
import { watch } from 'vue';

interface PersistOptions {
  key?: string;
  debounceTime?: number;
}

/**
 * Plugin to persist Pinia store state
 * This replaces localStorage direct usage with a more Vue-friendly approach
 */
export function createPersistedState(options: PersistOptions = {}) {
  return ({ store }: PiniaPluginContext) => {
    const storeKey = options.key || `pinia-${store.$id}`;
    const debounceTime = options.debounceTime || 500; // Default 500ms debounce
    let debounceTimer: ReturnType<typeof setTimeout> | null = null;
    
    // Initialize store with persisted state if available
    const persistedState = localStorage.getItem(storeKey);
    if (persistedState) {
      try {
        store.$patch(JSON.parse(persistedState));
      } catch (error) {
        console.error('Failed to parse stored state', error);
        // Clear corrupt entry
        localStorage.removeItem(storeKey);
      }
    }
    
    // Debounced function to persist state
    const persistState = (state: any) => {
      try {
        localStorage.setItem(storeKey, JSON.stringify(state));
      } catch (error) {
        console.error('Failed to save state to localStorage', error);
        // Handle quota exceeded or other storage errors
        if (error instanceof DOMException && 
            (error.name === 'QuotaExceededError' || 
             error.name === 'NS_ERROR_DOM_QUOTA_REACHED')) {
          console.warn('localStorage quota exceeded');
        }
      }
    };
    
    // Watch for store changes and persist them with debounce
    watch(
      () => store.$state,
      (state) => {
        if (debounceTimer) {
          clearTimeout(debounceTimer);
        }
        debounceTimer = setTimeout(() => {
          persistState(state);
        }, debounceTime);
      },
      { deep: true }
    );
  };
}

export default createPersistedState; 