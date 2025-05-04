<script lang="ts">
import NavBar from '@/components/NavBar.vue';
import { defineComponent, onMounted, ref } from 'vue';
import { useThemeStore } from './stores/theme';

export default defineComponent({
  components: { NavBar },
  setup() {
    const themeStore = useThemeStore();
    const forceDarkModeActive = ref(false);
    
    const forceDarkMode = () => {
      // Direct enforcement of dark mode
      document.documentElement.classList.add('dark');
      forceDarkModeActive.value = true;
      console.log("Forced dark mode applied");
      
      // Debug - check all dark mode classes
      const hasHtmlDark = document.documentElement.classList.contains('dark');
      console.log("HTML has dark class:", hasHtmlDark);
    };
    
    onMounted(() => {
      // Force theme application with a small delay to ensure it applies after rendering
      setTimeout(() => {
        themeStore.applyTheme();
        console.log("App mounted - applying theme");
        
        // Debug log for dark mode detection
        const isDarkMode = document.documentElement.classList.contains('dark');
        console.log("Is dark mode active:", isDarkMode);
        
        // Force dark mode again to be extra sure
        if (isDarkMode) {
          forceDarkMode();
        }
      }, 50);
    });
    
    return { themeStore, forceDarkModeActive };
  }
});
</script>

<template>
  <div class="min-h-screen transition-colors duration-200 bg-white dark:bg-gray-900 dark:text-white" :class="{ 'force-dark-mode': forceDarkModeActive }">
    <header>
      <NavBar />
    </header>

    <div class="py-4" />

    <div>
      <RouterView />
    </div>
  </div>
</template>

<style>
.force-dark-mode {
  background-color: #121212 !important;
}
.force-dark-mode header nav {
  background-color: #111827 !important;
}
</style>

<style module>
.container {
  @apply flex justify-start max-w-[1376px] mx-auto;
}
</style>
