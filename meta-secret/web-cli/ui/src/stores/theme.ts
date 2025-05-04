import { defineStore } from 'pinia';
import { ref, watch } from 'vue';

export const useThemeStore = defineStore('theme', () => {
  const theme = ref(localStorage.getItem('theme') || 'system');
  
  // Apply theme on initial load
  applyTheme();

  // Watch for system preference changes
  if (typeof window !== 'undefined') {
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  }
  
  // Watch for theme changes
  watch(theme, () => {
    localStorage.setItem('theme', theme.value);
    applyTheme();
    
    // Force CSS reapplication
    document.body.classList.add('theme-transition');
    setTimeout(() => {
      document.body.classList.remove('theme-transition');
    }, 300);
  });
  
  function applyTheme() {
    const isDark = 
      theme.value === 'dark' || 
      (theme.value === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    
    console.log('Applying theme, isDark:', isDark);
    
    if (isDark) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }
  
  return {
    theme,
    applyTheme
  };
}); 