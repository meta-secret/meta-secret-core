<template>
  <div class="container flex justify-center max-w-md pt-1 pb-4 items-stretch">
    <div class="flex items-center">
      <span class="text-gray-600 dark:text-gray-400 mr-2">Vault:</span>
      <h2 class="text-xl font-bold text-gray-800 dark:text-white py-1 px-2 border-b-2 border-orange-500">{{ vaultName }}</h2>
    </div>
  </div>

  <div class="container flex max-w-md py-2 items-stretch">
    <div class="flex w-full rounded-lg overflow-hidden shadow">
      <RouterLink
        :class="[
          'w-1/2 text-center py-3 px-6 text-white font-medium transition-colors',
          $route.path.includes('/vault/secrets')
            ? 'bg-orange-600'
            : 'bg-gray-700 hover:bg-gray-600',
        ]"
        to="/vault/secrets"
        >Secrets
      </RouterLink>

      <RouterLink
        :class="[
          'w-1/2 text-center py-3 px-6 text-white font-medium transition-colors',
          $route.path.includes('/vault/devices')
            ? 'bg-orange-600'
            : 'bg-gray-700 hover:bg-gray-600',
        ]"
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
