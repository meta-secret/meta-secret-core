<script setup lang="ts">
import { ref } from 'vue';
import { AppState } from '@/stores/app-state';

const appState = AppState();
const vaultName = ref(appState.getVaultName());
const deviceId = (appState.currState as any).device_id().wasm_id_str();
const showDeviceId = ref(false);

const toggleDeviceId = () => {
  showDeviceId.value = !showDeviceId.value;
};
</script>

<template>
  <div :class="$style.headerContainer">
    <div :class="$style.vaultBadge">
      <div :class="$style.vaultLabel">Vault Name</div>
      <div :class="$style.vaultSeparator"></div>
      <div :class="$style.vaultName">{{ vaultName }}</div>
      <button :class="$style.infoButton" @click="toggleDeviceId" title="Show Technical Information">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10"></circle>
          <line x1="12" y1="16" x2="12" y2="12"></line>
          <line x1="12" y1="8" x2="12.01" y2="8"></line>
        </svg>
      </button>
    </div>
    <div v-if="showDeviceId" :class="$style.deviceIdContainer">
      <span :class="$style.deviceIdLabel">Device Id:</span>
      <span :class="$style.deviceIdValue">{{ deviceId }}</span>
    </div>
  </div>

  <div :class="$style.navContainer">
    <div :class="$style.navWrapper">
      <RouterLink
        :class="[$style.navLink, $route.path.includes('/secrets') ? $style.activeLink : '']"
        to="/secrets"
        >Secrets
      </RouterLink>

      <RouterLink
        :class="[$style.navLink, $route.path.includes('/devices') ? $style.activeLink : '']"
        to="/devices"
        >Devices
      </RouterLink>
    </div>
  </div>

  <div>
    <RouterView />
  </div>
</template>

<style module>
.headerContainer {
  @apply container mx-auto flex flex-col items-center max-w-md pt-3 pb-4;
  position: relative;
}

.vaultBadge {
  @apply flex items-center gap-1;
  @apply px-4 py-2 rounded-full shadow-lg;
  @apply transition-all duration-300 hover:shadow-orange-900/20;
  @apply backdrop-blur-sm;
  @apply bg-white/90 border border-slate-300;
  box-shadow: 0 0 15px rgba(234, 88, 12, 0.2), inset 0 0 10px rgba(255, 255, 255, 0.3);
}

:global(.dark) .vaultBadge {
  @apply bg-slate-800/90 border-slate-700;
  box-shadow: 0 0 15px rgba(234, 88, 12, 0.2), inset 0 0 10px rgba(0, 0, 0, 0.3);
}

.vaultLabel {
  @apply text-sm text-slate-400 dark:text-slate-400 font-medium uppercase tracking-wider;
  @apply py-0.5;
}

.vaultSeparator {
  @apply w-0.5 h-6 mx-2 bg-gradient-to-b from-slate-700 via-orange-500 to-slate-700 rounded-full;
}

.vaultName {
  @apply text-xl font-bold bg-gradient-to-r from-orange-500 to-amber-500 bg-clip-text text-transparent;
  @apply py-0.5 px-2;
  text-shadow: 0 0 10px rgba(234, 88, 12, 0.3);
}

.infoButton {
  @apply text-slate-400 hover:text-orange-400 transition-all duration-300;
  @apply w-7 h-7 flex items-center justify-center rounded-full ml-1;
  @apply hover:bg-slate-700/50 hover:scale-110;
}

.deviceIdContainer {
  @apply mt-2 text-xs text-center py-2 px-4 rounded-lg;
  @apply backdrop-blur-sm;
  @apply animate-fadeIn;
  @apply bg-white/90 border border-slate-300/50 shadow-md shadow-black/10;
}

:global(.dark) .deviceIdContainer {
  @apply bg-slate-800/80 border-slate-700/50 shadow-black/30;
}

.deviceIdLabel {
  @apply mr-1 font-medium;
  @apply text-slate-600;
}

:global(.dark) .deviceIdLabel {
  @apply text-slate-300;
}

.deviceIdValue {
  @apply font-mono text-orange-300 font-medium;
}

.navContainer {
  @apply container mx-auto px-4 py-2;
}

.navWrapper {
  @apply flex mx-auto max-w-md rounded-lg overflow-hidden shadow;
}

.navLink {
  @apply w-1/2 text-center py-3 px-6 font-medium transition-colors;
  @apply bg-slate-200 text-slate-800 hover:bg-slate-300;
}

:global(.dark) .navLink {
  @apply bg-gray-700 text-white hover:bg-gray-600;
}

.activeLink {
  @apply bg-orange-500 text-white hover:bg-orange-500;
}

:global(.dark) .activeLink {
  @apply bg-orange-600 hover:bg-orange-600;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(-5px); }
  to { opacity: 1; transform: translateY(0); }
}

.animate-fadeIn {
  animation: fadeIn 0.2s ease-out forwards;
}
</style>
