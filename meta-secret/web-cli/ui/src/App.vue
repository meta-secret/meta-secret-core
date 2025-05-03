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

<style>
.container {
  display: flex;
  justify-content: flex-start;
  max-width: 1376px;
  margin: 0 auto;
}
</style>
