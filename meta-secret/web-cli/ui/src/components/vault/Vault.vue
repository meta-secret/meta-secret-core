<script setup lang="ts">
import { ref } from 'vue';
import { AppState } from '@/stores/app-state';

const appState = AppState();
const vaultName = ref(appState.getVaultName());
const deviceId = (appState.currState as any).device_id().wasm_id_str();
</script>

<template>
  <div :class="$style.headerContainer">
    <div :class="$style.headerContent">
      <span :class="$style.labelText">Vault:</span>
      <h2 :class="$style.vaultTitle">{{ vaultName }}</h2>
    </div>
    <div :class="$style.deviceIdContainer">
      <span :class="$style.deviceIdLabel">Device ID:</span>
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
  @apply container mx-auto flex flex-col items-center max-w-md pt-1 pb-4;
}

.headerContent {
  @apply flex items-center;
}

.labelText {
  @apply text-2xl text-slate-600 dark:text-slate-50 mr-2 font-medium;
}

.vaultTitle {
  @apply text-lg font-medium text-zinc-600 py-1 px-4 rounded-md;
  @apply dark:text-yellow-200 dark:bg-gradient-to-r dark:from-orange-500 dark:to-amber-500;
  @apply shadow-none dark:shadow-orange-400/30;
  @apply border-t-2 border-b-2 border-orange-500 dark:border-orange-300;
  @apply transition-all duration-200;
  @apply dark:animate-pulse;
  animation-duration: 3s;
}

.deviceIdContainer {
  @apply mt-2 text-sm text-center;
}

.deviceIdLabel {
  @apply text-slate-500 dark:text-slate-400 mr-1;
}

.deviceIdValue {
  @apply font-mono text-slate-700 dark:text-slate-300;
}

.navContainer {
  @apply container mx-auto px-4 py-2;
}

.navWrapper {
  @apply flex mx-auto max-w-md rounded-lg overflow-hidden shadow;
}

.navLink {
  @apply w-1/2 text-center py-3 px-6 text-white font-medium transition-colors bg-gray-700 hover:bg-gray-600;
}

.activeLink {
  @apply bg-orange-600 hover:bg-orange-600;
}
</style>
