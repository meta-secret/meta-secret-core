<script lang="ts">
import NavBar from '@/components/NavBar.vue';
import PasskeyAuth from '@/components/PasskeyAuth.vue';
import { defineComponent } from 'vue';

export default defineComponent({
  components: {
    NavBar,
    PasskeyAuth
  },
});
</script>

<template>
  <div id="app" class="app-shell">
    <NavBar />
    <div id="content">
      <router-view />
    </div>
    <PasskeyAuth />
  </div>
</template>

<style>
.app-shell {
  min-height: 100vh;
  background: #080f1c;
}
</style>
