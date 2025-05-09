<script lang="ts">
import NavBar from '@/components/NavBar.vue';
import { defineComponent, onMounted, ref, watch } from 'vue';
import { useThemeStore } from './stores/theme';

export default defineComponent({
  components: {
    NavBar,
  },
  setup() {
    const themeStore = useThemeStore();
    const forceDarkModeActive = ref(false);

    // Watch for changes to the isDarkMode computed property
    watch(
      () => themeStore.isDarkMode,
      (isDark) => {
        applyTheme(isDark);
      },
      { immediate: true },
    );

    // Apply theme on initial mount
    onMounted(() => {
      applyTheme(themeStore.isDarkMode);
    });

    // Method to apply theme class
    function applyTheme(isDark: boolean) {
      const html = document.documentElement;
      if (isDark) {
        html.classList.add('dark');
        forceDarkModeActive.value = true;
        console.log('Forced dark mode applied');

        // Debug - check all dark mode classes
        const hasHtmlDark = document.documentElement.classList.contains('dark');
        console.log('HTML has dark class:', hasHtmlDark);
      } else {
        html.classList.remove('dark');
        forceDarkModeActive.value = false;
        console.log('Forced dark mode removed');
      }
    }

    return { themeStore, forceDarkModeActive };
  },
});
</script>

<template>
  <div
    id="app"
    class="min-h-screen bg-white dark:bg-gray-900 transition-colors duration-200"
    :class="{ 'force-dark-mode': forceDarkModeActive }"
  >
    <NavBar />
    <div id="content">
      <router-view />
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

