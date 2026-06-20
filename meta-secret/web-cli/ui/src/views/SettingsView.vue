<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { AppState } from '@/stores/app-state';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog';
import { Separator } from '@/components/ui/separator';
import { ArrowLeft, TriangleAlert } from 'lucide-vue-next';

const router = useRouter();
const jsAppState = AppState();
const isCleaning = ref(false);

async function cleanDatabase() {
  if (isCleaning.value) return;
  isCleaning.value = true;
  try {
    await jsAppState.cleanDatabase();
    await jsAppState.appStateInit();
    await router.push('/');
  } finally {
    isCleaning.value = false;
  }
}
</script>

<template>
  <div class="mx-auto max-w-lg px-4 py-8">
    <div class="mb-6 flex items-center gap-3">
      <Button variant="ghost" size="icon" @click="router.push('/')">
        <ArrowLeft class="h-4 w-4" />
      </Button>
      <h1 class="flex-1 text-center text-2xl font-bold">Settings</h1>
      <Badge variant="destructive" class="text-[10px] uppercase tracking-widest">Alpha</Badge>
    </div>

    <Separator class="mb-6" />

    <section class="space-y-4">
      <h2 class="text-sm font-semibold uppercase tracking-widest text-muted-foreground">Data Management</h2>

      <Card>
        <CardHeader class="flex flex-row items-center gap-3 border-b pb-4">
          <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-destructive/10 text-destructive">
            <TriangleAlert class="h-5 w-5" />
          </div>
          <CardTitle class="text-base">Clean Database</CardTitle>
        </CardHeader>
        <CardContent class="pt-4">
          <p class="text-sm text-muted-foreground">
            Delete all vault data and start fresh. This action removes all secrets, vault configurations, and resets the
            application to its initial state.
          </p>
          <div class="mt-4 flex justify-end">
            <AlertDialog>
              <AlertDialogTrigger as-child>
                <Button variant="destructive">Clean Database</Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Are you absolutely sure?</AlertDialogTitle>
                  <AlertDialogDescription>
                    This action cannot be undone. All vault data, secrets, and configurations will be permanently
                    deleted.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>Cancel</AlertDialogCancel>
                  <AlertDialogAction
                    class="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                    :disabled="isCleaning"
                    @click="cleanDatabase"
                  >
                    {{ isCleaning ? 'Cleaning...' : 'Yes, Clean Database' }}
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </div>
        </CardContent>
      </Card>
    </section>
  </div>
</template>
