<script setup lang="ts">
import NavBar from '@/components/NavBar.vue';
import PasskeyAuth from '@/components/PasskeyAuth.vue';
import { useThemeStore } from '@/stores/theme';
import { computed } from 'vue';
import { useRoute } from 'vue-router';
import { AppState } from '@/stores/app-state';
import { registrationSurfaceActive } from '@/features/registration/registrationSurface';
import { Toaster } from '@/components/ui/sonner';

// Initialise theme store on mount — applies .dark to <html> immediately
useThemeStore();
const route = useRoute();
const appState = AppState();
const registrationEntry = computed(
  () => route.matched.some((record) => record.path === '/') && (!appState.isMember || registrationSurfaceActive.value),
);
</script>

<template>
  <div class="min-h-screen bg-background text-foreground">
    <NavBar v-if="!registrationEntry" />
    <div id="content">
      <router-view />
    </div>
    <PasskeyAuth />
    <Toaster />
  </div>
</template>
