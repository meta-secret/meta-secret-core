<script setup lang="ts">
import { defineComponent } from 'vue';
import { AppState } from '@/stores/app-state';


</script>

<template>
  <div class="container flex justify-center max-w-md py-2 items-stretch">
    <p class="flex">{{ this.getVaultName() }}</p>
  </div>

  <div class="container flex max-w-md py-2 items-stretch">
    <RouterLink
      class="w-1/2 text-center rounded-l-lg px-6 py-3 text-white bg-orange-600 active:bg-orange-800"
      to="/vault/secrets"
      >Secrets
    </RouterLink>

    <RouterLink
      class="w-1/2 text-center rounded-r-lg px-6 py-3 text-dark bg-gray-100 active:bg-gray-300"
      to="/vault/devices"
      >Devices
    </RouterLink>
  </div>

  <div>
    <RouterView />
  </div>
</template>
