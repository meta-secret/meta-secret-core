<script setup lang="ts">
import Documentation from '@/components/Documentation.vue';
</script>

<template>
  <div class="flex flex-col content-center items-center">
    <h1 class="text-2xl">Documentation</h1>
  </div>

  <div class="py-4"></div>

  <div class="container mx-auto">
    <div class="container flex flex-col items-center justify-center">
      <div class="py-4"></div>
      <div class="max-w-3xl">
        <Documentation />
      </div>
    </div>
  </div>
</template> 