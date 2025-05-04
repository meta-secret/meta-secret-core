<script lang="ts">
import NavBar from '@/components/NavBar.vue';
import { defineComponent, onMounted } from 'vue';
import { useThemeStore } from './stores/theme';

export default defineComponent({
  components: { NavBar },
  setup() {
    const themeStore = useThemeStore();
    
    onMounted(() => {
      themeStore.applyTheme();
      console.log("App mounted - applying theme");
      
      // Debug log for dark mode detection
      const isDarkMode = document.documentElement.classList.contains('dark');
      console.log("Is dark mode active:", isDarkMode);
    });
    
    return { themeStore };
  }
});
</script>

<template>
  <div class="min-h-screen transition-colors duration-200 bg-white dark:bg-gray-900 dark:text-white">
    <header>
      <NavBar />
    </header>

    <div class="py-4" />

    <div>
      <RouterView />
    </div>
  </div>
</template>

<style module>
.container {
  @apply flex justify-start max-w-[1376px] mx-auto;
}
</style>
