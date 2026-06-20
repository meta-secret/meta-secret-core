import { defineStore } from 'pinia';
import { ref, computed, watch } from 'vue';
import type { Ref, ComputedRef } from 'vue';

type ThemeOption = 'light' | 'dark' | 'system';

export interface ThemeState {
  theme: Ref<ThemeOption>;
  isDarkMode: ComputedRef<boolean>;
  setTheme: (theme: ThemeOption) => void;
}

function applyDarkClass(dark: boolean) {
  document.documentElement.classList.toggle('dark', dark);
}

export const useThemeStore = defineStore('theme', (): ThemeState => {
  const savedTheme = localStorage.getItem('theme') as ThemeOption | null;
  const theme = ref<ThemeOption>(savedTheme || 'dark');

  const mediaQuery =
    typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null;

  const isDarkMode = computed(() => {
    if (!mediaQuery) return false;
    return theme.value === 'dark' || (theme.value === 'system' && mediaQuery.matches);
  });

  // React to OS-level preference changes when theme is 'system'
  mediaQuery?.addEventListener('change', () => {
    if (theme.value === 'system') {
      applyDarkClass(mediaQuery.matches);
    }
  });

  watch(
    isDarkMode,
    (dark) => {
      localStorage.setItem('theme', theme.value);
      applyDarkClass(dark);
    },
    { immediate: true },
  );

  function setTheme(newTheme: ThemeOption) {
    theme.value = newTheme;
  }

  return { theme, isDarkMode, setTheme };
});
