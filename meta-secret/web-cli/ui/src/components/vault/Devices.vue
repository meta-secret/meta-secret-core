<script setup lang="ts">
import { computed } from 'vue';
import { AppState } from '@/stores/app-state';
import Device from '@/components/vault/Device.vue';
import { UserDataOutsiderStatus } from 'meta-secret-web-cli';

const appState = AppState();
const users = computed(() => (appState.currState as any).as_vault().as_member().vault_data().users());

const memberDevices = computed(() => users.value.filter((membership: any) => membership.is_member()));
const declinedDevices = computed(() => users.value.filter((membership: any) => {
  if (!membership.is_outsider()) return false;
  return membership.as_outsider().status === UserDataOutsiderStatus.Declined;
}));
const pendingDevices = computed(() => users.value.filter((membership: any) => {
  if (!membership.is_outsider()) return false;
  return membership.as_outsider().status === UserDataOutsiderStatus.Pending;
}));
</script>

<template>
  <div class="main-content">
    <div class="page-wide">
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
            <div class="section-header">Members</div>
            <div v-for="membership in memberDevices" :key="membership.user_data().device.device_id.wasm_id_str()">
              <Device :membership="membership" />
            </div>
          </div>

          <div v-if="pendingDevices.length > 0">
            <div class="section-header">Pending Requests</div>
            <div v-for="membership in pendingDevices" :key="membership.user_data().device.device_id.wasm_id_str()">
              <Device :membership="membership" />
            </div>
          </div>

          <div v-if="declinedDevices.length > 0">
            <div class="section-header">Declined Devices</div>
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

.section-header {
  padding: 8px 20px 6px;
  border-bottom: 1px solid #1a2840;
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.08em;
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

  .section-header {
    padding: 8px 16px;
  }
}
</style>
