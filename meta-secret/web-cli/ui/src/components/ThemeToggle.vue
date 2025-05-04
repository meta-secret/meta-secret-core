<script setup>
import { ref, computed, onMounted, onBeforeUnmount } from 'vue';
import { useThemeStore } from '../stores/theme';
import { SunIcon, MoonIcon, DesktopComputerIcon } from '@heroicons/vue/outline';

const themeStore = useThemeStore();
const isOpen = ref(false);
const dropdownRef = ref(null);

const currentTheme = computed(() => themeStore.theme);

const toggleDropdown = () => {
  isOpen.value = !isOpen.value;
};

const setTheme = (theme) => {
  // Direct approach to theme switching
  console.log('Setting theme to:', theme);
  
  // Update localStorage
  localStorage.setItem('theme', theme);
  
  // Update store value
  themeStore.theme = theme;
  
  // Direct DOM manipulation to ensure theme change
  const isDark = 
    theme === 'dark' || 
    (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
  
  if (isDark) {
    document.documentElement.classList.add('dark');
  } else {
    document.documentElement.classList.remove('dark');
  }
  
  // Also call store's apply function as backup
  themeStore.applyTheme();
  
  isOpen.value = false;
};

const handleClickOutside = (event) => {
  if (dropdownRef.value && !dropdownRef.value.contains(event.target)) {
    isOpen.value = false;
  }
};

// Add click outside listener when component is mounted
onMounted(() => {
  document.addEventListener('click', handleClickOutside);
  console.log('Current theme on mount:', currentTheme.value);
});

// Remove listener when component is unmounted
onBeforeUnmount(() => {
  document.removeEventListener('click', handleClickOutside);
});
</script>

<template>
  <div class="relative" ref="dropdownRef">
    <button 
      @click.stop="toggleDropdown" 
      class="flex items-center text-gray-900 dark:text-gray-100 bg-gray-200 dark:bg-transparent border border-gray-300 dark:border-transparent hover:bg-gray-300 dark:hover:bg-gray-700 px-3 py-2 rounded-md text-sm font-medium"
      title="Theme settings"
    >
      <SunIcon v-if="currentTheme === 'light'" class="h-5 w-5" />
      <MoonIcon v-else-if="currentTheme === 'dark'" class="h-5 w-5" />
      <DesktopComputerIcon v-else class="h-5 w-5" />
    </button>
    
    <div 
      v-if="isOpen"
      class="absolute right-0 z-10 mt-2 w-40 origin-top-right rounded-md bg-white dark:bg-gray-800 shadow-lg ring-1 ring-black ring-opacity-5 focus:outline-none"
    >
      <div class="py-1">
        <a 
          href="#" 
          @click.prevent="setTheme('light')" 
          class="flex items-center px-4 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"
        >
          <SunIcon class="h-5 w-5 mr-2" />
          Light
        </a>
        <a 
          href="#" 
          @click.prevent="setTheme('dark')" 
          class="flex items-center px-4 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"
        >
          <MoonIcon class="h-5 w-5 mr-2" />
          Dark
        </a>
        <a 
          href="#" 
          @click.prevent="setTheme('system')" 
          class="flex items-center px-4 py-2 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700"
        >
          <DesktopComputerIcon class="h-5 w-5 mr-2" />
          System
        </a>
      </div>
    </div>
  </div>
</template>

<style module>
.container {
  @apply relative;
}

.themeButton {
  @apply flex items-center text-gray-900 dark:text-gray-100;
  @apply bg-gray-200 dark:bg-transparent;
  @apply border border-gray-300 dark:border-transparent;
  @apply hover:bg-gray-300 dark:hover:bg-gray-700;
  @apply px-3 py-2 rounded-md text-sm font-medium;
}

.icon {
  @apply h-5 w-5;
}

.dropdown {
  @apply absolute right-0 z-10 mt-2 w-40 origin-top-right;
  @apply rounded-md bg-white dark:bg-gray-800 shadow-lg;
  @apply ring-1 ring-black ring-opacity-5 focus:outline-none;
}

.dropdownContent {
  @apply py-1;
}

.menuItem {
  @apply flex items-center px-4 py-2 text-sm;
  @apply text-gray-700 dark:text-gray-200;
  @apply hover:bg-gray-100 dark:hover:bg-gray-700;
}

.menuIcon {
  @apply h-5 w-5 mr-2;
}
</style> 