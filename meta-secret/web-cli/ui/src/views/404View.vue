<script setup lang="ts">
import { useRouter } from 'vue-router';
import { Button } from '@/components/ui/button';

const router = useRouter();
</script>

<template>
  <div class="flex min-h-[calc(100vh-3.5rem)] flex-col items-center justify-center gap-4 p-6">
    <h1 class="text-4xl font-bold">404</h1>
    <p class="text-muted-foreground">Page Not Found</p>
    <Button variant="outline" @click="router.push('/')">Go Home</Button>
  </div>
</template>
