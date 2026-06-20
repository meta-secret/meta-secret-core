<script setup lang="ts">
import { computed } from 'vue';
import { AppState } from '@/stores/app-state';
import Device from '@/components/vault/Device.vue';
import { UserDataOutsiderStatus } from 'meta-secret-web-cli';
import { vaultSecrets } from '@/locales/en';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';

const appState = AppState();
const users = computed(() => (appState.currState as any).as_vault().as_member().vault_data().users());

const memberDevices = computed(() => users.value.filter((m: any) => m.is_member()));
const pendingDevices = computed(() =>
  users.value.filter((m: any) => m.is_outsider() && m.as_outsider().status === UserDataOutsiderStatus.Pending),
);
const declinedDevices = computed(() =>
  users.value.filter((m: any) => m.is_outsider() && m.as_outsider().status === UserDataOutsiderStatus.Declined),
);

const requiredDevicesToSafety = computed(() => 3 - users.value.length);
const shouldShowDevicesWarning = computed(() => requiredDevicesToSafety.value > 0);
</script>

<template>
  <div class="mx-auto max-w-6xl px-4 py-10 md:px-6">
    <Alert v-if="shouldShowDevicesWarning" class="mb-4">
      <AlertDescription>
        ⚠ {{ vaultSecrets.warningPrefix }} {{ requiredDevicesToSafety }} {{ vaultSecrets.warningMiddle }}
      </AlertDescription>
    </Alert>

    <Card>
      <CardHeader class="border-b pb-4">
        <CardTitle>Devices</CardTitle>
        <p class="mt-0.5 text-xs font-semibold uppercase tracking-widest text-muted-foreground">
          Detailed information about user devices
        </p>
      </CardHeader>

      <CardContent class="p-0">
        <p v-if="users.length === 0" class="py-8 text-center text-sm text-muted-foreground">
          No devices connected yet
        </p>
        <template v-else>
          <Device
            v-for="membership in memberDevices"
            :key="membership.user_data().device.device_id.wasm_id_str()"
            :membership="membership"
          />
          <Device
            v-for="membership in pendingDevices"
            :key="membership.user_data().device.device_id.wasm_id_str()"
            :membership="membership"
          />
          <Device
            v-for="membership in declinedDevices"
            :key="membership.user_data().device.device_id.wasm_id_str()"
            :membership="membership"
          />
        </template>
      </CardContent>
    </Card>
  </div>
</template>
