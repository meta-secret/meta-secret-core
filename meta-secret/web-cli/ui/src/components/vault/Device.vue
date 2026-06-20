<script setup lang="ts">
import { computed, ref } from 'vue';
import {
  DeviceData,
  DeviceUiCategory,
  JoinActionUpdate,
  UserData,
  UserDataOutsiderStatus,
  WasmUserMembership,
} from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { getAppManager, getDeviceId } from '@/utils/wasmBridge';
import { vaultDevices } from '@/locales/en';
import { deviceCategoryLabel } from '@/utils/deviceCategoryLabel';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog';
import { Shield } from 'lucide-vue-next';

const props = defineProps<{ membership: WasmUserMembership }>();

const appState = AppState();
const appManager = getAppManager();

const user = computed<UserData>(() => props.membership.user_data());
const device = computed<DeviceData>(() => props.membership.user_data().device);
const deviceName = computed(() => device.value.device_name.as_str());

const deviceDisplay = computed(() => {
  try {
    return { category: device.value.ui_category(), unavailable: false as const };
  } catch {
    return { category: DeviceUiCategory.Other, unavailable: true as const };
  }
});

const typeLabel = computed(() =>
  deviceCategoryLabel(deviceDisplay.value.category, deviceDisplay.value.unavailable, vaultDevices),
);

const currentDeviceId = computed(() => {
  try {
    return getDeviceId(appState.currState);
  } catch {
    return '';
  }
});

const deviceId = computed(() => device.value.device_id.wasm_id_str());
const isCurrent = computed(() => deviceId.value === currentDeviceId.value);
const vaultName = computed(() => appState.getVaultName() || vaultDevices.fallbackVaultName);
const isMember = computed(() => props.membership.is_member());
const isPending = computed(
  () => props.membership.is_outsider() && props.membership.as_outsider().status === UserDataOutsiderStatus.Pending,
);
const isDeclined = computed(
  () => props.membership.is_outsider() && props.membership.as_outsider().status === UserDataOutsiderStatus.Declined,
);

const isJoinConfirmOpen = ref(false);
const isSubmitting = ref(false);

const handleAccept = async () => {
  if (isSubmitting.value) return;
  isSubmitting.value = true;
  try {
    await appManager.update_membership(user.value, JoinActionUpdate.Accept);
    await appState.updateState();
    isJoinConfirmOpen.value = false;
  } finally {
    isSubmitting.value = false;
  }
};

const handleDecline = async () => {
  if (isSubmitting.value) return;
  isSubmitting.value = true;
  try {
    await appManager.update_membership(user.value, JoinActionUpdate.Decline);
    await appState.updateState();
    isJoinConfirmOpen.value = false;
  } finally {
    isSubmitting.value = false;
  }
};
</script>

<template>
  <div
    class="flex items-center gap-3 border-b px-5 py-4 last:border-0 transition-colors"
    :class="[isPending && 'cursor-pointer hover:bg-muted/50', isDeclined && 'opacity-60']"
    :role="isPending ? 'button' : undefined"
    :tabindex="isPending ? 0 : undefined"
    @click="isPending && (isJoinConfirmOpen = true)"
    @keydown.enter.prevent="isPending && (isJoinConfirmOpen = true)"
    @keydown.space.prevent="isPending && (isJoinConfirmOpen = true)"
  >
    <!-- Device icon -->
    <div class="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-muted text-primary">
      <svg
        v-if="deviceDisplay.category === DeviceUiCategory.Iphone"
        width="18"
        height="22"
        viewBox="0 0 60 59"
        fill="none"
      >
        <rect
          x="14"
          y="5"
          width="32"
          height="50"
          rx="6"
          fill="currentColor"
          opacity="0.15"
          stroke="currentColor"
          stroke-width="2"
        />
        <rect x="24" y="-1" width="13" height="11" rx="2" fill="currentColor" opacity="0.5" />
      </svg>
      <svg
        v-else-if="deviceDisplay.category === DeviceUiCategory.Android"
        width="18"
        height="22"
        viewBox="0 0 60 59"
        fill="none"
      >
        <rect
          x="14"
          y="5"
          width="32"
          height="50"
          rx="6"
          fill="currentColor"
          opacity="0.15"
          stroke="currentColor"
          stroke-width="2"
        />
        <circle cx="30" cy="10" r="2.2" fill="currentColor" />
      </svg>
      <svg
        v-else-if="deviceDisplay.category === DeviceUiCategory.Desktop"
        width="28"
        height="18"
        viewBox="0 0 54 36"
        fill="none"
      >
        <rect
          x="5.5"
          y="1.5"
          width="44"
          height="33"
          rx="3.3"
          fill="currentColor"
          opacity="0.15"
          stroke="currentColor"
          stroke-width="2"
        />
        <path
          d="M0 31.2H54V33.4C54 34.6 53 35.6 51.8 35.6H4.4C2 35.6 0 33.7 0 31.2Z"
          fill="currentColor"
          opacity="0.5"
        />
      </svg>
      <svg
        v-else-if="deviceDisplay.category === DeviceUiCategory.Web"
        width="20"
        height="20"
        viewBox="0 0 54 54"
        fill="none"
      >
        <circle cx="27" cy="27" r="24" fill="currentColor" opacity="0.15" stroke="currentColor" stroke-width="2" />
        <line x1="3" y1="27" x2="51" y2="27" stroke="currentColor" stroke-width="1.5" />
        <line x1="27" y1="3" x2="27" y2="51" stroke="currentColor" stroke-width="1.5" />
      </svg>
      <svg
        v-else-if="deviceDisplay.category === DeviceUiCategory.Cli"
        width="26"
        height="18"
        viewBox="0 0 58 44"
        fill="none"
      >
        <rect
          x="1"
          y="1"
          width="56"
          height="42"
          rx="3"
          fill="currentColor"
          opacity="0.15"
          stroke="currentColor"
          stroke-width="2"
        />
        <polyline
          points="8,22 14,28 8,34"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          fill="none"
        />
        <line x1="17" y1="28" x2="33" y2="28" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
      </svg>
      <svg v-else width="20" height="20" viewBox="0 0 56 56" fill="none">
        <circle cx="28" cy="28" r="20" fill="currentColor" opacity="0.15" stroke="currentColor" stroke-width="2" />
      </svg>
    </div>

    <!-- Info -->
    <div class="flex-1">
      <p class="font-semibold">{{ deviceName }}</p>
      <p class="text-xs text-muted-foreground">{{ typeLabel }}</p>
    </div>

    <!-- Status badges -->
    <Badge v-if="isMember && isCurrent" variant="outline" class="border-primary text-primary">
      {{ vaultDevices.statusCurrent }}
    </Badge>
    <Badge v-else-if="isMember" class="bg-green-500/15 text-green-600 hover:bg-green-500/15">
      {{ vaultDevices.statusMember }}
    </Badge>
    <Badge v-if="isPending" class="bg-yellow-500/15 text-yellow-600 hover:bg-yellow-500/15">
      {{ vaultDevices.statusPending }}
    </Badge>
    <Badge v-if="isDeclined" variant="destructive">
      {{ vaultDevices.statusDeclined }}
    </Badge>
  </div>

  <!-- Join confirm dialog -->
  <Dialog
    :open="isJoinConfirmOpen"
    @update:open="
      (v) => {
        if (!v && !isSubmitting) isJoinConfirmOpen = false;
      }
    "
  >
    <DialogContent class="max-w-md text-center">
      <div class="flex flex-col items-center gap-4 py-2">
        <div class="flex h-14 w-14 items-center justify-center rounded-full bg-primary/10">
          <Shield class="h-6 w-6 text-primary" />
        </div>
        <DialogHeader>
          <DialogTitle>
            {{ vaultDevices.confirmJoinPrefix }}
            <span class="text-primary">{{ typeLabel }}</span>
            {{ vaultDevices.confirmJoinMiddle }}
            <span class="font-bold">{{ vaultName }}</span
            >?
          </DialogTitle>
          <DialogDescription>{{ typeLabel }}</DialogDescription>
        </DialogHeader>
        <DialogFooter class="w-full gap-2 sm:gap-2">
          <Button variant="outline" class="flex-1" :disabled="isSubmitting" @click="handleDecline">
            {{ vaultDevices.actionDecline }}
          </Button>
          <Button class="flex-1" :disabled="isSubmitting" @click="handleAccept">
            {{ vaultDevices.actionAccept }}
          </Button>
        </DialogFooter>
      </div>
    </DialogContent>
  </Dialog>
</template>
