<script setup lang="ts">
import { computed, ref } from 'vue';
import { component_core_version, component_db_version, component_server_version } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { getDeviceId } from '@/utils/wasmBridge';
import { vaultComponentVersions, vaultTechnicalInfo } from '@/locales/en';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { Box, Cloud, Database, Info, LayoutPanelLeft, Monitor } from 'lucide-vue-next';

const appState = AppState();
const vaultName = computed(() => appState.getVaultName());
const deviceId = computed(() => getDeviceId(appState.currState));
const showDeviceId = ref(false);

const webUiVersion = __WEB_UI_VERSION__;
const coreVersion = computed(() => component_core_version());
const serverVersion = computed(() => component_server_version());
const dbVersion = computed(() => component_db_version());

const versionRows = computed(() => [
  { label: vaultComponentVersions.labelWebUi, value: webUiVersion, icon: Monitor },
  { label: vaultComponentVersions.labelCore, value: coreVersion.value, icon: Box },
  { label: vaultComponentVersions.labelServer, value: serverVersion.value, icon: Cloud },
  { label: vaultComponentVersions.labelDb, value: dbVersion.value, icon: Database },
]);
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
        :class="showDeviceId && 'bg-muted text-foreground'"
        :title="vaultTechnicalInfo.title"
        @click="showDeviceId = !showDeviceId"
      >
        <Info class="h-4 w-4" :class="showDeviceId ? 'text-foreground' : 'text-muted-foreground'" />
      </Button>
    </div>

    <Transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0 -translate-y-2 scale-[0.98]"
      enter-to-class="opacity-100 translate-y-0 scale-100"
      leave-active-class="transition duration-150 ease-in"
      leave-from-class="opacity-100 translate-y-0 scale-100"
      leave-to-class="opacity-0 -translate-y-1 scale-[0.98]"
    >
      <Card
        v-if="showDeviceId"
        class="mx-auto mt-3 max-w-md border-muted/60 bg-card/80 shadow-sm backdrop-blur-sm"
      >
        <CardHeader class="pb-3">
          <CardTitle class="flex items-center gap-2 text-sm font-medium">
            <LayoutPanelLeft class="h-4 w-4 text-muted-foreground" />
            {{ vaultTechnicalInfo.title }}
          </CardTitle>
        </CardHeader>
        <CardContent class="space-y-4 pt-0">
          <div class="space-y-1.5">
            <p class="text-xs text-muted-foreground">{{ vaultTechnicalInfo.labelDeviceId.replace(':', '') }}</p>
            <p class="rounded-lg border bg-muted/40 px-3 py-2 font-mono text-xs leading-relaxed break-all text-foreground">
              {{ deviceId }}
            </p>
          </div>

          <Separator />

          <div class="space-y-2">
            <p class="text-xs text-muted-foreground">{{ vaultComponentVersions.sectionTitle }}</p>
            <dl class="space-y-1.5">
              <div
                v-for="row in versionRows"
                :key="row.label"
                class="flex items-center justify-between gap-3 rounded-lg px-2 py-1.5 transition-colors hover:bg-muted/30"
              >
                <dt class="flex items-center gap-2 text-sm text-muted-foreground">
                  <component :is="row.icon" class="h-3.5 w-3.5 shrink-0 opacity-70" />
                  {{ row.label }}
                </dt>
                <dd>
                  <Badge variant="secondary" class="font-mono text-[11px] font-normal tabular-nums">
                    {{ row.value }}
                  </Badge>
                </dd>
              </div>
            </dl>
          </div>
        </CardContent>
      </Card>
    </Transition>

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
