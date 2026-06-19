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
import { vaultDevices } from '@/locales/en';
import { deviceCategoryLabel } from '@/utils/deviceCategoryLabel';

const props = defineProps<{ membership: WasmUserMembership }>();

const appState = AppState();
const appManager = appState.appManager as any;

const user = computed<UserData>(() => props.membership.user_data());
const device = computed<DeviceData>(() => props.membership.user_data().device);

const deviceName = computed(() => device.value.device_name.as_str());

const deviceDisplay = computed(() => {
  try {
    const category = device.value.ui_category();
    return { category, unavailable: false as const };
  } catch {
    return {
      category: DeviceUiCategory.Other,
      unavailable: true as const,
    };
  }
});

const typeLabel = computed(() =>
  deviceCategoryLabel(deviceDisplay.value.category, deviceDisplay.value.unavailable, vaultDevices),
);

const currentDeviceId = computed(() => {
  try {
    return (appState.currState as any).device_id().wasm_id_str();
  } catch {
    return '';
  }
});

const deviceId = computed(() => device.value.device_id.wasm_id_str());
const isCurrent = computed(() => deviceId.value === currentDeviceId.value);

const vaultName = computed(() => appState.getVaultName() || vaultDevices.fallbackVaultName);

const isMember = computed(() => props.membership.is_member());
const isPending = computed(() => {
  if (!props.membership.is_outsider()) return false;
  return props.membership.as_outsider().status === UserDataOutsiderStatus.Pending;
});
const isDeclined = computed(() => {
  if (!props.membership.is_outsider()) return false;
  return props.membership.as_outsider().status === UserDataOutsiderStatus.Declined;
});
const isJoinConfirmOpen = ref(false);
const isSubmitting = ref(false);

const openJoinConfirm = () => {
  if (!isPending.value || isSubmitting.value) return;
  isJoinConfirmOpen.value = true;
};

const closeJoinConfirm = () => {
  if (isSubmitting.value) return;
  isJoinConfirmOpen.value = false;
};

const accept = async () => {
  await appManager.update_membership(user.value, JoinActionUpdate.Accept);
  await appState.updateState();
};

const decline = async () => {
  await appManager.update_membership(user.value, JoinActionUpdate.Decline);
  await appState.updateState();
};

const handleAccept = async () => {
  if (isSubmitting.value) return;
  isSubmitting.value = true;
  try {
    await accept();
    isJoinConfirmOpen.value = false;
  } finally {
    isSubmitting.value = false;
  }
};

const handleDecline = async () => {
  if (isSubmitting.value) return;
  isSubmitting.value = true;
  try {
    await decline();
    isJoinConfirmOpen.value = false;
  } finally {
    isSubmitting.value = false;
  }
};
</script>

<template>
  <div
    class="device-row"
    :class="{ declined: isDeclined, clickable: isPending }"
    :role="isPending ? 'button' : undefined"
    :tabindex="isPending ? 0 : undefined"
    @click="openJoinConfirm"
    @keydown.enter.prevent="openJoinConfirm"
    @keydown.space.prevent="openJoinConfirm"
  >
    <div class="device-icon-box">
      <svg
        v-if="deviceDisplay.category === DeviceUiCategory.Iphone"
        width="22"
        height="22"
        viewBox="0 0 60 59"
        fill="none"
      >
        <rect x="14" y="5" width="32" height="50" rx="6" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2" />
        <rect x="24" y="-1" width="13" height="11" rx="2" fill="#91BDFF" opacity="0.7" />
      </svg>
      <svg
        v-else-if="deviceDisplay.category === DeviceUiCategory.Android"
        width="22"
        height="22"
        viewBox="0 0 60 59"
        fill="none"
      >
        <rect x="14" y="5" width="32" height="50" rx="6" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2" />
        <circle cx="30" cy="10" r="2.2" fill="#91BDFF" />
        <circle cx="23" cy="50" r="2" fill="#91BDFF" />
        <circle cx="30" cy="50" r="2" fill="#91BDFF" />
        <circle cx="37" cy="50" r="2" fill="#91BDFF" />
      </svg>
      <svg
        v-else-if="deviceDisplay.category === DeviceUiCategory.Tablet"
        width="18"
        height="22"
        viewBox="0 0 42 50"
        fill="none"
      >
        <rect x="1" y="1" width="40" height="48" rx="5" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2" />
      </svg>
      <svg
        v-else-if="deviceDisplay.category === DeviceUiCategory.Desktop"
        width="32"
        height="22"
        viewBox="0 0 54 36"
        fill="none"
      >
        <rect x="5.5" y="1.5" width="44" height="33" rx="3.3" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2.2" />
        <path d="M0 31.2H54V33.4C54 34.6 53 35.6 51.8 35.6H4.4C2 35.6 0 33.7 0 31.2Z" fill="#91BDFF" />
      </svg>
      <svg
        v-else-if="deviceDisplay.category === DeviceUiCategory.Web"
        width="22"
        height="22"
        viewBox="0 0 54 54"
        fill="none"
      >
        <circle cx="27" cy="27" r="24" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2" />
        <line x1="3" y1="16" x2="51" y2="16" stroke="#91BDFF" stroke-width="1.6" />
        <line x1="3" y1="27" x2="51" y2="27" stroke="#91BDFF" stroke-width="1.6" />
        <line x1="3" y1="38" x2="51" y2="38" stroke="#91BDFF" stroke-width="1.6" />
        <line x1="27" y1="3" x2="27" y2="51" stroke="#91BDFF" stroke-width="1.6" />
      </svg>
      <svg
        v-else-if="deviceDisplay.category === DeviceUiCategory.Cli"
        width="30"
        height="22"
        viewBox="0 0 58 44"
        fill="none"
      >
        <rect x="1" y="1" width="56" height="42" rx="3" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2" />
        <polyline
          points="8,22 14,28 8,34"
          stroke="#91BDFF"
          stroke-width="2.2"
          stroke-linecap="round"
          stroke-linejoin="round"
          fill="none"
        />
        <line x1="17" y1="28" x2="33" y2="28" stroke="#91BDFF" stroke-width="2.2" stroke-linecap="round" />
      </svg>
      <svg v-else width="22" height="22" viewBox="0 0 56 56" fill="none">
        <path d="M23 26 A5 5 0 0 0 33 26" stroke="#91BDFF" stroke-width="2.2" stroke-linecap="round" fill="none" />
        <path d="M18 26 A10 10 0 0 0 38 26" stroke="#91BDFF" stroke-width="2.2" stroke-linecap="round" fill="none" />
        <path d="M13 26 A15 15 0 0 0 43 26" stroke="#91BDFF" stroke-width="2.2" stroke-linecap="round" fill="none" />
        <path
          d="M8 28H48A4 4 0 0 1 52 32V50A4 4 0 0 1 48 54H8A4 4 0 0 1 4 50V32A4 4 0 0 1 8 28Z"
          fill="#1a2e4a"
          stroke="#3b7eff"
          stroke-width="2"
        />
      </svg>
    </div>

    <div class="device-info">
      <div class="device-name">{{ deviceName }}</div>
      <div class="device-model">{{ typeLabel }}</div>
    </div>

    <span v-if="isMember && isCurrent" class="badge-current">{{ vaultDevices.statusCurrent }}</span>
    <span v-else-if="isMember" class="badge-member">{{ vaultDevices.statusMember }}</span>
    <span v-if="isPending" class="badge-pending">{{ vaultDevices.statusPending }}</span>
    <span v-if="isDeclined" class="badge-declined">{{ vaultDevices.statusDeclined }}</span>
  </div>

  <div v-if="isJoinConfirmOpen" class="confirm-overlay" @click="closeJoinConfirm">
    <div class="confirm-dialog" @click.stop>
      <div class="confirm-icon-box">
        <svg width="28" height="28" viewBox="0 0 24 24" fill="none">
          <path
            d="M12 3L4 7v5c0 5.25 3.5 10.15 8 11.5C16.5 22.15 20 17.25 20 12V7L12 3z"
            fill="#1a3a8f"
            stroke="#60a5fa"
            stroke-width="1.5"
            stroke-linejoin="round"
          />
          <circle cx="12" cy="12" r="2.5" fill="#60a5fa" />
        </svg>
      </div>
      <div class="confirm-title">
        {{ vaultDevices.confirmJoinPrefix }}
        <span class="accent-device">{{ typeLabel }}</span>
        {{ vaultDevices.confirmJoinMiddle }}
        <span class="accent-vault">{{ vaultName }}</span
        >?
      </div>
      <div class="confirm-subtitle">{{ typeLabel }}</div>
      <div class="confirm-actions">
        <button class="btn-decline" :disabled="isSubmitting" @click="handleDecline">
          {{ vaultDevices.actionDecline }}
        </button>
        <button class="btn-accept" :disabled="isSubmitting" @click="handleAccept">
          {{ vaultDevices.actionAccept }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.device-row {
  padding: 16px 20px;
  border-bottom: 1px solid #1a2840;
  display: flex;
  align-items: center;
  gap: 14px;
}

.device-row.clickable {
  cursor: pointer;
  transition: background 0.15s ease;
}

.device-row.clickable:hover {
  background: #101f33;
}

.device-row.declined {
  opacity: 0.7;
}

.device-row:last-child {
  border-bottom: none;
}

.device-icon-box {
  width: 42px;
  height: 42px;
  border-radius: 10px;
  background: #111e30;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.device-info {
  flex: 1;
}

.device-name {
  font-size: 15px;
  font-weight: 700;
  color: #ffffff;
}

.device-model {
  font-size: 12px;
  color: #3a5070;
  margin-top: 2px;
}

.badge-member {
  background: #0d2e20;
  border: 1px solid #1a5a36;
  color: #34d399;
  font-size: 11px;
  font-weight: 700;
  border-radius: 20px;
  padding: 3px 12px;
}

.badge-current {
  background: #1a2e4a;
  border: 1px solid #2563eb;
  color: #60a5fa;
  font-size: 11px;
  font-weight: 700;
  border-radius: 20px;
  padding: 3px 12px;
}

.badge-declined {
  background: #3a1820;
  border: 1px solid #a3213d;
  color: #f87171;
  font-size: 11px;
  font-weight: 700;
  border-radius: 20px;
  padding: 3px 12px;
}

.badge-pending {
  background: #2a1a05;
  border: 1px solid #7a4410;
  color: #f59e42;
  font-size: 11px;
  font-weight: 700;
  border-radius: 20px;
  padding: 3px 12px;
}

.confirm-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.65);
  backdrop-filter: blur(6px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 200;
  padding: 24px;
}

.confirm-dialog {
  background: #0d1726;
  border: 1px solid #1e3050;
  border-radius: 20px;
  padding: 28px;
  width: 100%;
  max-width: 520px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 18px;
  text-align: center;
  box-shadow: 0 32px 80px rgba(0, 0, 0, 0.6);
}

.confirm-icon-box {
  width: 60px;
  height: 60px;
  border-radius: 50%;
  background: #1a2e4a;
  display: flex;
  align-items: center;
  justify-content: center;
}

.confirm-title {
  font-size: 18px;
  font-weight: 800;
  color: #ffffff;
  line-height: 1.35;
}

.accent-device {
  color: #60a5fa;
}

.accent-vault {
  color: #2563eb;
}

.confirm-subtitle {
  font-size: 13px;
  color: #4a6080;
}

.confirm-actions {
  display: flex;
  gap: 10px;
  width: 100%;
}

.btn-decline,
.btn-accept {
  flex: 1;
  border: none;
  border-radius: 12px;
  height: 46px;
  font-size: 15px;
  font-weight: 700;
  cursor: pointer;
}

.btn-decline {
  background: #111e30;
  color: #8aaacf;
  border: 1px solid #1e3050;
}

.btn-accept {
  background: #2563eb;
  color: #ffffff;
}

.btn-decline:disabled,
.btn-accept:disabled {
  opacity: 0.65;
  cursor: wait;
}
</style>
