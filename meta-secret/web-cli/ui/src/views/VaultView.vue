<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
import { useRouter } from 'vue-router';
import RegistrationComponent from '@/components/vault/auth/Registration.vue';
import VaultComponent from '@/components/vault/Vault.vue';
import AlphaBadge from '@/components/common/AlphaBadge.vue';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';

const router = useRouter();
const jsAppState = AppState();
const authStore = useAuthStore();
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

function navigateToSettings() {
  router.push('/settings');
}

// Monitor authentication state and initialize the app when auth is complete
watch(() => authStore.isAuthenticated, async (isAuthenticated) => {
  if (isAuthenticated && authStore.masterKey) {
    try {
      await jsAppState.appStateInit();
      isInitialized.value = true;
    } catch (error) {
      console.error('Error initializing app state:', error);
    }
  }
}, { immediate: true });

onMounted(() => {
  // Add global click listener for closing the settings menu
  document.addEventListener('click', handleClickOutside);
});
</script>

<template>
  <div class="py-3"></div>
  <div :class="$style.headerContainer">
    <AlphaBadge />
  </div>

  <div class="flex justify-center items-center py-6 relative">
    <p class="text-2xl font-bold text-gray-900 dark:text-gray-100">Decentralized Secret Manager</p>
    
    <!-- Information Button -->
    <button 
      :class="$style.infoButton" 
      @click="router.push('/info')"
      aria-label="Information"
    >
      <span class="text-2xl">ℹ️</span>
    </button>
    
    <!-- Settings Icon Button -->
    <button 
      id="settings-button"
      :class="$style.settingsButton" 
      @click="navigateToSettings"
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

  <!-- Show loading while authentication is complete but app isn't initialized yet -->
  <div v-if="authStore.isAuthenticated && !isInitialized" class="text-center mt-8">
    <p class="text-gray-400">Loading Vault Information...</p>
  </div>

  <!-- Only show registration or vault when authenticated and initialized -->
  <div v-else-if="authStore.isAuthenticated && isInitialized && !jsAppState.isMember">
    <RegistrationComponent />
    
    <!-- Information Card -->
    <div class="max-w-lg mx-auto mt-6 p-4 bg-blue-50 dark:bg-blue-900 rounded-lg shadow">
      <div class="flex items-center justify-between">
        <div>
          <h3 class="font-medium text-blue-800 dark:text-blue-200">New to Meta Secret?</h3>
          <p class="text-sm text-blue-700 dark:text-blue-300 mt-1">
            Learn how Meta Secret works and how to use it effectively
          </p>
        </div>
        <button 
          @click="router.push('/info')"
          class="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 transition-colors"
        >
          Learn More
        </button>
      </div>
    </div>
  </div>
  <div v-else-if="authStore.isAuthenticated && isInitialized && jsAppState.isMember">
    <VaultComponent />
  </div>
</template>

<style module>
.headerContainer {
  @apply container mx-auto flex flex-col items-center max-w-md;
  position: relative;
}

.infoButton {
  @apply ml-2 flex items-center justify-center rounded-full;
  @apply hover:bg-gray-100 dark:hover:bg-gray-700;
  @apply transition-colors duration-200 ease-in-out;
  width: 44px;
  height: 44px;
}

.settingsButton {
  @apply ml-2 flex items-center justify-center rounded-full;
  @apply hover:bg-gray-100 dark:hover:bg-gray-700;
  @apply transition-colors duration-200 ease-in-out;
  width: 44px;
  height: 44px;
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