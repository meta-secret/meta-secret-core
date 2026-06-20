<script setup lang="ts">
import NavBar from '@/components/NavBar.vue';
import PasskeyAuth from '@/components/PasskeyAuth.vue';
import { useThemeStore } from '@/stores/theme';
import { Toaster } from '@/components/ui/sonner';

// Initialise theme store on mount — applies .dark to <html> immediately
useThemeStore();
</script>

<template>
  <div class="min-h-screen bg-background text-foreground">
    <NavBar />
    <div id="content">
      <router-view />
    </div>
    <PasskeyAuth />
    <Toaster />
  </div>
</template>
