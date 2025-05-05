import { defineStore } from 'pinia';
import { ref, computed, Ref, ComputedRef } from 'vue';

type ThemeOption = 'light' | 'dark' | 'system';

export interface ThemeState {
  theme: Ref<ThemeOption>;
  isDarkMode: ComputedRef<boolean>;
  setTheme: (theme: ThemeOption) => void;
}

export const useThemeStore = defineStore('theme', (): ThemeState => {
  // Store theme state in pinia, default to system
  const theme = ref<ThemeOption>('system');

  // Computed property to determine if dark mode should be applied
  const isDarkMode = computed(() => {
    if (typeof window === 'undefined') {
      // Default to false during SSR
      return false;
    }
    
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    return theme.value === 'dark' || (theme.value === 'system' && mediaQuery.matches);
  });

  // Watch for system preference changes
  if (typeof window !== 'undefined') {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');

    mediaQuery.addEventListener('change', () => {
      if (theme.value === 'system') {
        // The isDarkMode computed property will automatically update
        console.log('System preference changed');
      }
    });
  }

  // Change the theme
  function setTheme(newTheme: ThemeOption) {
    console.log(`Changing theme to ${newTheme}`);
    theme.value = newTheme;
  }

  return {
    theme,
    isDarkMode,
    setTheme,
  };
});
