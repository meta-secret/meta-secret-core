<template>
  <div :class="$style.headerContainer">
    <div :class="$style.headerContent">
      <span :class="$style.labelText">Vault:</span>
      <h2 :class="$style.vaultTitle">{{ vaultName }}</h2>
    </div>
  </div>

  <div :class="$style.navContainer">
    <div :class="$style.navWrapper">
      <RouterLink
        :class="[$style.navLink, $route.path.includes('/vault/secrets') ? $style.activeLink : '']"
        to="/vault/secrets"
        >Secrets
      </RouterLink>

      <RouterLink
        :class="[$style.navLink, $route.path.includes('/vault/devices') ? $style.activeLink : '']"
        to="/vault/devices"
        >Devices
      </RouterLink>
    </div>
  </div>

  <div>
    <RouterView />
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue';
import { AppState } from '@/stores/app-state';

const appState = AppState();
const vaultName = ref('');

onMounted(async () => {
  vaultName.value = await appState.getVaultName();
});
</script>

<style module>
.headerContainer {
  @apply container mx-auto flex justify-center max-w-md pt-1 pb-4;
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
  @apply shadow-md dark:shadow-orange-400/30;
  @apply border-b-2 border-orange-500 dark:border-orange-300;
  @apply transition-all duration-200;
  @apply dark:animate-pulse;
  animation-duration: 3s;
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
