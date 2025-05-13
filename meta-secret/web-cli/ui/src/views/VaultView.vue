<script setup lang="ts">
import { ref, onMounted } from 'vue';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';

import { AppState } from '@/stores/app-state';

const jsAppState = AppState();
const isInitialized = ref(false);
const isCleaning = ref(false);
const showSettings = ref(false);

async function cleanDatabase() {
  if (isCleaning.value) return;
  if (!confirm('Are you sure you want to delete all vault data and start over? This action cannot be undone.')) return;
  isCleaning.value = true;
  try {
    await (jsAppState.appManager as any).clean_up_database();
    await jsAppState.appStateInit();
  } finally {
    isCleaning.value = false;
    showSettings.value = false;
  }
}

function toggleSettings() {
  showSettings.value = !showSettings.value;
  
  if (showSettings.value) {
    // Position the menu after Vue updates the DOM
    setTimeout(() => {
      const button = document.getElementById('settings-button');
      const menu = document.getElementById('settings-menu');
      if (button && menu) {
        const buttonRect = button.getBoundingClientRect();
        menu.style.position = 'fixed';
        menu.style.top = (buttonRect.bottom + 5) + 'px';
        menu.style.left = (buttonRect.left + buttonRect.width/2 - menu.offsetWidth/2) + 'px';
      }
    }, 0);
  }
}

// Close settings menu when clicking outside
function handleClickOutside(event: MouseEvent) {
  const settingsMenu = document.getElementById('settings-menu');
  const settingsButton = document.getElementById('settings-button');
  if (
    settingsMenu && 
    settingsButton && 
    !settingsMenu.contains(event.target as Node) && 
    !settingsButton.contains(event.target as Node)
  ) {
    showSettings.value = false;
  }
}

onMounted(async () => {
  await jsAppState.appStateInit();
  isInitialized.value = true;
  
  // Add global click listener for closing the settings menu
  document.addEventListener('click', handleClickOutside);
});
</script>

<template>
  <div class="py-3"></div>
  <div :class="$style.headerContainer">
    <div :class="$style.alphaBadge">Alpha Version</div>
  </div>

  <div class="flex justify-center items-center py-6 relative">
    <p class="text-2xl font-bold text-gray-900 dark:text-gray-100">Decentralized Secret Manager</p>
    
    <!-- Settings Icon Button -->
    <button 
      id="settings-button"
      :class="$style.settingsButton" 
      @click.stop="toggleSettings"
      aria-label="Settings"
    >
      <span class="text-2xl">⚙️</span>
    </button>
    
    <!-- Settings Dropdown Menu -->
    <div 
      v-if="showSettings" 
      id="settings-menu"
      :class="$style.settingsMenu"
    >
      <button
        :class="$style.menuItem"
        :disabled="isCleaning"
        @click="cleanDatabase"
      >
        <span v-if="isCleaning">Cleaning...</span>
        <span v-else>🧹 Clean Database</span>
      </button>
    </div>
  </div>

  <div v-if="!isInitialized" class="text-center mt-8">
    <p class="text-gray-400">Loading Vault Information...</p>
  </div>

  <div v-else-if="!jsAppState.isMember">
    <RegistrationComponent />
  </div>
  <div v-else-if="jsAppState.isMember">
    <VaultComponent />
  </div>
</template>

<style module>
.headerContainer {
  @apply container mx-auto flex flex-col items-center max-w-md;
  position: relative;
}

.alphaBadge {
  @apply absolute -top-1 right-2 bg-red-500 text-white text-xs font-bold px-2 py-0.5 rounded;
  @apply shadow-md shadow-red-900/100;
  @apply uppercase tracking-wide;
  font-size: 0.65rem;
  transform: rotate(5deg);
}

.settingsButton {
  @apply ml-2 flex items-center justify-center rounded-full;
  @apply hover:bg-gray-100 dark:hover:bg-gray-700;
  @apply transition-colors duration-200 ease-in-out;
  width: 44px;
  height: 44px;
}

.cleanButton {
  @apply ml-4 px-3 py-1 text-sm bg-white dark:bg-gray-800 rounded-md;
  @apply text-gray-700 dark:text-gray-200 border border-gray-200 dark:border-gray-700;
  @apply hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors duration-150;
  @apply disabled:opacity-50 disabled:cursor-not-allowed flex items-center;
}

.settingsMenu {
  @apply absolute mt-2 w-48 bg-white dark:bg-gray-800 shadow-lg;
  @apply rounded-md overflow-hidden z-50 border border-gray-200 dark:border-gray-700;
  position: fixed;
  top: auto;
  right: auto;
  transform: none;
}

.menuItem {
  @apply w-full px-4 py-2 text-left text-sm text-gray-700 dark:text-gray-200;
  @apply hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors duration-150;
  @apply disabled:opacity-50 disabled:cursor-not-allowed flex items-center;
}

.menuItem span {
  @apply flex-1;
}
</style>