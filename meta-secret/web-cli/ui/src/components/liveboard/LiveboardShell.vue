<script setup lang="ts">
import '@/design/liveboard/tokens.css';
import '@/design/liveboard/liveboard.css';
</script>
<template>
  <main class="ms-liveboard liveboard-shell" aria-label="MetaSecret Liveboard">
    <div class="app"><slot /></div>
    <slot name="dialogs" />
  </main>
</template>
<style scoped>
.liveboard-shell {
  min-height: 100dvh;
  line-height: normal;
}
</style>
