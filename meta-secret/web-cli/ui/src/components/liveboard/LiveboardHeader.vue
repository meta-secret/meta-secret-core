<script setup lang="ts">
defineProps<{ vaultName: string }>();
</script>
<template>
  <header class="app-header">
    <div>
      <p class="eyebrow">YOUR VAULT</p>
      <h1>MetaSecret</h1>
    </div>
    <span class="vault-name">{{ vaultName }}</span>
  </header>
</template>
