<script setup lang="ts">
import { Card, CardContent } from '@/components/ui/card';
</script>

<template>
  <div class="flex min-h-[calc(100vh-3.5rem)] items-center justify-center p-6">
    <Card class="w-full max-w-sm text-center">
      <CardContent class="pt-6">
        <h1 class="mb-2 text-xl font-bold">Contact</h1>
        <a href="mailto:contact@meta-secret.org" class="text-primary hover:underline">
          contact@meta-secret.org
        </a>
      </CardContent>
    </Card>
  </div>
</template>
