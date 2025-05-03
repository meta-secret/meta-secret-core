<template>
  <div class="container flex justify-center max-w-md pt-1 pb-4 items-stretch">
    <div class="flex items-center">
      <span class="text-gray-600 mr-2">Vault:</span>
      <h2 class="text-xl font-bold text-gray-800 py-1 px-2 border-b-2 border-orange-500">{{ vaultName }}</h2>
    </div>
  </div>

  <div class="container flex max-w-md py-2 items-stretch">
    <RouterLink
      :class="[
        'w-1/2 text-center rounded-l-lg px-6 py-3',
        $route.path.includes('/vault/secrets')
          ? 'text-white bg-orange-600 active:bg-orange-800'
          : 'text-dark bg-gray-100 active:bg-gray-300',
      ]"
      to="/vault/secrets"
      >Secrets
    </RouterLink>

    <RouterLink
      :class="[
        'w-1/2 text-center rounded-r-lg px-6 py-3',
        $route.path.includes('/vault/devices')
          ? 'text-white bg-orange-600 active:bg-orange-800'
          : 'text-dark bg-gray-100 active:bg-gray-300',
      ]"
      to="/vault/devices"
      >Devices
    </RouterLink>
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
