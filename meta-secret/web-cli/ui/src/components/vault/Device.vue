<script setup lang="ts">
import { computed } from 'vue';
import {
  DeviceData,
  JoinActionUpdate,
  UserData,
  UserDataOutsiderStatus,
  WasmUserMembership,
} from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';

const props = defineProps<{ membership: WasmUserMembership }>();

const appState = AppState();
const appManager = appState.appManager as any;

const user = computed<UserData>(() => props.membership.user_data());
const device = computed<DeviceData>(() => props.membership.user_data().device);

const deviceName = computed(() => device.value.device_name.as_str());
const deviceTypeRaw = computed(() => {
  try {
    return device.value.device_type.as_str();
  } catch {
    return 'Other';
  }
});

const currentDeviceId = computed(() => {
  try {
    return (appState.currState as any).device_id().wasm_id_str();
  } catch {
    return '';
  }
});

const deviceId = computed(() => device.value.device_id.wasm_id_str());
const isCurrent = computed(() => deviceId.value === currentDeviceId.value);

const normalizedType = computed(() => {
  const value = String(deviceTypeRaw.value || '').toLowerCase();
  if (value.includes('android')) return 'android';
  if (value.includes('iphone') || value.includes('ios')) return 'iphone';
  if (value.includes('tablet') || value.includes('ipad')) return 'tablet';
  if (value.includes('desktop') || value.includes('laptop') || value.includes('mac') || value.includes('windows')) return 'desktop';
  if (value.includes('cli') || value.includes('terminal')) return 'cli';
  if (value.includes('web') || value.includes('browser')) return 'web';
  return 'other';
});

const typeLabel = computed(() => {
  const mapping: Record<string, string> = {
    android: 'Android',
    iphone: 'iPhone',
    tablet: 'Tablet',
    desktop: 'Laptop',
    cli: 'CLI',
    web: 'Web',
    other: 'Other',
  };
  return mapping[normalizedType.value] || 'Other';
});

const isMember = computed(() => props.membership.is_member());
const isPending = computed(() => {
  if (!props.membership.is_outsider()) return false;
  return props.membership.as_outsider().status === UserDataOutsiderStatus.Pending;
});
const isDeclined = computed(() => {
  if (!props.membership.is_outsider()) return false;
  return props.membership.as_outsider().status === UserDataOutsiderStatus.Declined;
});

const accept = async () => {
  await appManager.update_membership(user.value, JoinActionUpdate.Accept);
  await appState.updateState();
};

const decline = async () => {
  await appManager.update_membership(user.value, JoinActionUpdate.Decline);
  await appState.updateState();
};
</script>

<template>
  <div class="device-row" :class="{ declined: isDeclined }">
    <div class="device-icon-box">
      <svg v-if="normalizedType === 'iphone'" width="22" height="22" viewBox="0 0 60 59" fill="none">
        <rect x="14" y="5" width="32" height="50" rx="6" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2"/>
        <rect x="24" y="-1" width="13" height="11" rx="2" fill="#91BDFF" opacity="0.7"/>
      </svg>
      <svg v-else-if="normalizedType === 'android'" width="22" height="22" viewBox="0 0 60 59" fill="none">
        <rect x="14" y="5" width="32" height="50" rx="6" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2"/>
        <circle cx="30" cy="10" r="2.2" fill="#91BDFF"/>
        <circle cx="23" cy="50" r="2" fill="#91BDFF"/>
        <circle cx="30" cy="50" r="2" fill="#91BDFF"/>
        <circle cx="37" cy="50" r="2" fill="#91BDFF"/>
      </svg>
      <svg v-else-if="normalizedType === 'tablet'" width="18" height="22" viewBox="0 0 42 50" fill="none">
        <rect x="1" y="1" width="40" height="48" rx="5" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2"/>
      </svg>
      <svg v-else-if="normalizedType === 'desktop'" width="32" height="22" viewBox="0 0 54 36" fill="none">
        <rect x="5.5" y="1.5" width="44" height="33" rx="3.3" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2.2"/>
        <path d="M0 31.2H54V33.4C54 34.6 53 35.6 51.8 35.6H4.4C2 35.6 0 33.7 0 31.2Z" fill="#91BDFF"/>
      </svg>
      <svg v-else-if="normalizedType === 'web'" width="22" height="22" viewBox="0 0 54 54" fill="none">
        <circle cx="27" cy="27" r="24" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2"/>
        <line x1="3" y1="16" x2="51" y2="16" stroke="#91BDFF" stroke-width="1.6"/>
        <line x1="3" y1="27" x2="51" y2="27" stroke="#91BDFF" stroke-width="1.6"/>
        <line x1="3" y1="38" x2="51" y2="38" stroke="#91BDFF" stroke-width="1.6"/>
        <line x1="27" y1="3" x2="27" y2="51" stroke="#91BDFF" stroke-width="1.6"/>
      </svg>
      <svg v-else-if="normalizedType === 'cli'" width="30" height="22" viewBox="0 0 58 44" fill="none">
        <rect x="1" y="1" width="56" height="42" rx="3" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2"/>
        <polyline points="8,22 14,28 8,34" stroke="#91BDFF" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
        <line x1="17" y1="28" x2="33" y2="28" stroke="#91BDFF" stroke-width="2.2" stroke-linecap="round"/>
      </svg>
      <svg v-else width="22" height="22" viewBox="0 0 56 56" fill="none">
        <path d="M23 26 A5 5 0 0 0 33 26" stroke="#91BDFF" stroke-width="2.2" stroke-linecap="round" fill="none"/>
        <path d="M18 26 A10 10 0 0 0 38 26" stroke="#91BDFF" stroke-width="2.2" stroke-linecap="round" fill="none"/>
        <path d="M13 26 A15 15 0 0 0 43 26" stroke="#91BDFF" stroke-width="2.2" stroke-linecap="round" fill="none"/>
        <path d="M8 28H48A4 4 0 0 1 52 32V50A4 4 0 0 1 48 54H8A4 4 0 0 1 4 50V32A4 4 0 0 1 8 28Z" fill="#1a2e4a" stroke="#3b7eff" stroke-width="2"/>
      </svg>
    </div>

    <div class="device-info">
      <div class="device-name">{{ deviceName }}</div>
      <div class="device-model">{{ typeLabel }}</div>
    </div>

    <span v-if="isMember && isCurrent" class="badge-current">Current</span>
    <span v-else-if="isMember" class="badge-member">Member</span>
    <span v-if="isDeclined" class="badge-declined">Declined</span>

    <div v-if="isPending" class="action-buttons">
      <button class="accept-btn" @click="accept">Accept</button>
      <button class="decline-btn" @click="decline">Decline</button>
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

.action-buttons {
  display: flex;
  gap: 8px;
}

.accept-btn,
.decline-btn {
  border: none;
  border-radius: 10px;
  height: 32px;
  padding: 0 14px;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.accept-btn {
  background: #1f9f55;
  color: #ffffff;
}

.decline-btn {
  background: #c0392b;
  color: #ffffff;
}
</style>
