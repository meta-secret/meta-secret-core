<script setup lang="ts">
import { computed, ref } from 'vue';
import { component_core_version, component_db_version, component_server_version } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { getDeviceId } from '@/utils/wasmBridge';
import { vaultComponentVersions, vaultTechnicalInfo } from '@/locales/en';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { Info } from 'lucide-vue-next';

const appState = AppState();
const vaultName = computed(() => appState.getVaultName());
const deviceId = computed(() => getDeviceId(appState.currState));
const showDeviceId = ref(false);

const webUiVersion = __WEB_UI_VERSION__;
const coreVersion = computed(() => component_core_version());
const serverVersion = computed(() => component_server_version());
const dbVersion = computed(() => component_db_version());
</script>

<template>
  <div class="mx-auto max-w-2xl px-4 pt-6">
    <!-- Vault pill + info toggle -->
    <div class="flex items-center justify-center gap-2">
      <div class="flex items-center gap-3 rounded-full border bg-card px-6 py-2.5 shadow-sm">
        <span class="text-[10px] font-bold uppercase tracking-widest text-muted-foreground">Vault Name</span>
        <Separator orientation="vertical" class="h-4" />
        <span class="font-bold text-foreground">{{ vaultName }}</span>
      </div>
      <Button
        variant="ghost"
        size="icon"
        class="h-8 w-8"
        :title="vaultTechnicalInfo.title"
        @click="showDeviceId = !showDeviceId"
      >
        <Info class="h-4 w-4 text-muted-foreground" />
      </Button>
    </div>

    <!-- Technical info panel -->
    <div v-if="showDeviceId" class="mt-3 flex flex-col items-center gap-1 text-xs text-muted-foreground">
      <span class="font-semibold uppercase tracking-widest">{{ vaultTechnicalInfo.title }}</span>
      <div class="flex gap-2">
        <span>{{ vaultTechnicalInfo.labelDeviceId }}</span>
        <code class="font-mono">{{ deviceId }}</code>
      </div>
      <Separator class="my-1 w-48" />
      <span class="font-semibold uppercase tracking-widest">{{ vaultComponentVersions.sectionTitle }}</span>
      <div
        v-for="[label, val] in [
          [vaultComponentVersions.labelWebUi, webUiVersion],
          [vaultComponentVersions.labelCore, coreVersion],
          [vaultComponentVersions.labelServer, serverVersion],
          [vaultComponentVersions.labelDb, dbVersion],
        ]"
        :key="label"
        class="flex gap-2"
      >
        <span>{{ label }}</span>
        <code class="font-mono">{{ val }}</code>
      </div>
    </div>

    <!-- Tab bar -->
    <div class="mt-4 flex rounded-xl border bg-muted p-1 gap-1">
      <RouterLink
        to="/secrets"
        class="flex flex-1 items-center justify-center rounded-lg px-4 py-2 text-sm font-semibold transition-all"
        :class="
          $route.path.includes('/secrets') || $route.path === '/'
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'
        "
      >
        Secrets
      </RouterLink>
      <RouterLink
        to="/devices"
        class="flex flex-1 items-center justify-center rounded-lg px-4 py-2 text-sm font-semibold transition-all"
        :class="
          $route.path.includes('/devices')
            ? 'bg-background text-foreground shadow-sm'
            : 'text-muted-foreground hover:text-foreground'
        "
      >
        Devices
      </RouterLink>
    </div>

    <RouterView />
  </div>
</template>
