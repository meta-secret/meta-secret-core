<script setup lang="ts">
import { computed } from 'vue';
import { AppState } from '@/stores/app-state';
import Device from '@/components/vault/Device.vue';
import { UserDataOutsiderStatus } from 'meta-secret-web-cli';
import { vaultSecrets } from '@/locales/en';

const appState = AppState();
const users = computed(() => (appState.currState as any).as_vault().as_member().vault_data().users());

const memberDevices = computed(() => users.value.filter((membership: any) => membership.is_member()));
const declinedDevices = computed(() =>
  users.value.filter((membership: any) => {
    if (!membership.is_outsider()) return false;
    return membership.as_outsider().status === UserDataOutsiderStatus.Declined;
  }),
);
const pendingDevices = computed(() =>
  users.value.filter((membership: any) => {
    if (!membership.is_outsider()) return false;
    return membership.as_outsider().status === UserDataOutsiderStatus.Pending;
  }),
);

const currentDeviceCount = computed(() => users.value.length);
const requiredDevicesToSafety = computed(() => 3 - currentDeviceCount.value);
const shouldShowDevicesWarning = computed(() => requiredDevicesToSafety.value > 0);
</script>

<template>
  <div class="main-content">
    <div class="page-wide">
      <div v-if="shouldShowDevicesWarning" class="warning-banner">
        <span class="warning-icon">⚠</span>
        <span> {{ vaultSecrets.warningPrefix }} {{ requiredDevicesToSafety }} {{ vaultSecrets.warningMiddle }} </span>
      </div>

      <div class="card">
        <div class="card-header">
          <div>
            <div class="card-title">Devices</div>
            <div class="section-sub">Detailed information about user devices</div>
          </div>
        </div>

        <div v-if="users.length === 0" class="empty-state">No devices connected yet</div>

        <template v-else>
          <div v-if="memberDevices.length > 0">
            <div v-for="membership in memberDevices" :key="membership.user_data().device.device_id.wasm_id_str()">
              <Device :membership="membership" />
            </div>
          </div>

          <div v-if="pendingDevices.length > 0">
            <div v-for="membership in pendingDevices" :key="membership.user_data().device.device_id.wasm_id_str()">
              <Device :membership="membership" />
            </div>
          </div>

          <div v-if="declinedDevices.length > 0">
            <div v-for="membership in declinedDevices" :key="membership.user_data().device.device_id.wasm_id_str()">
              <Device :membership="membership" />
            </div>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.main-content {
  padding: 48px 24px;
  display: flex;
  justify-content: center;
}

.page-wide {
  width: 100%;
  max-width: 1240px;
}

.warning-banner {
  background: #1a2518;
  border: 1px solid #2a3a1e;
  border-radius: 12px;
  padding: 12px 16px;
  color: #8aaa70;
  font-size: 13px;
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}

.warning-icon {
  color: #e6b44a;
}

.card {
  background: #0d1726;
  border: 1px solid #1a2840;
  border-radius: 16px;
  overflow: hidden;
}

.card-header {
  padding: 18px 20px;
  border-bottom: 1px solid #1a2840;
}

.card-title {
  font-size: 17px;
  line-height: 1.2;
  font-weight: 700;
  color: #ffffff;
}

.section-sub {
  margin-top: 2px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: #3a5070;
}

.empty-state {
  padding: 30px;
  color: #8aaacf;
  font-size: 14px;
  text-align: center;
}

@media (max-width: 900px) {
  .main-content {
    padding: 24px 16px;
  }

  .card {
    border-radius: 14px;
  }

  .card-header {
    padding: 16px;
  }

  .card-title {
    font-size: 16px;
  }

  .section-sub {
    font-size: 10px;
  }
}
</style>
