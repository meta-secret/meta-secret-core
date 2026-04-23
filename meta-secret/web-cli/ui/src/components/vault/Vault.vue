<script setup lang="ts">
import { computed, ref } from 'vue';
import { AppState } from '@/stores/app-state';

const appState = AppState();
const vaultName = computed(() => appState.getVaultName());
const deviceId = computed(() => (appState.currState as any).device_id().wasm_id_str());
const showDeviceId = ref(false);

const toggleDeviceId = () => {
  showDeviceId.value = !showDeviceId.value;
};
</script>

<template>
  <div class="vault-shell">
    <div class="vault-pill-wrap">
      <div class="vault-pill">
        <span class="vault-name-label">Vault Name</span>
        <div class="vault-separator"></div>
        <span class="vault-name-value">{{ vaultName }}</span>
      </div>
      <button class="vault-info-btn" title="Show Technical Information" @click="toggleDeviceId">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
          <circle cx="12" cy="12" r="9" stroke="#4a6080" stroke-width="2" />
          <path d="M12 8v1M12 11v5" stroke="#4a6080" stroke-width="2" stroke-linecap="round"/>
        </svg>
      </button>
    </div>

    <div v-if="showDeviceId" class="device-id-container">
      <span class="device-id-label">Device Id:</span>
      <span class="device-id-value">{{ deviceId }}</span>
    </div>

    <div class="tab-wrap">
      <RouterLink :class="['tab-btn', $route.path.includes('/secrets') ? 'active' : 'inactive']" to="/secrets">
        Secrets
      </RouterLink>
      <RouterLink :class="['tab-btn', $route.path.includes('/devices') ? 'active' : 'inactive']" to="/devices">
        Devices
      </RouterLink>
    </div>

    <RouterView />
  </div>
</template>

<style scoped>
.vault-shell {
  padding: 24px 24px 0;
}

.vault-pill-wrap {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 10px;
}

.vault-pill {
  display: flex;
  align-items: center;
  gap: 14px;
  background: #0d1726;
  border: 1px solid #1e3050;
  border-radius: 50px;
  padding: 10px 24px;
  width: fit-content;
  box-shadow: 0 0 0 1px #2563eb22 inset;
}

.vault-name-label {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.1em;
  color: #3a5070;
  text-transform: uppercase;
}

.vault-separator {
  width: 1px;
  height: 18px;
  background: #1e3050;
}

.vault-name-value {
  font-size: 16px;
  font-weight: 800;
  color: #2563eb;
}

.vault-info-btn {
  width: 34px;
  height: 34px;
  border-radius: 8px;
  border: 1px solid #1a2840;
  background: #111e30;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

.vault-info-btn:hover {
  border-color: #2563eb44;
}

.device-id-container {
  margin-top: 10px;
  display: flex;
  justify-content: center;
  align-items: center;
  gap: 6px;
}

.device-id-label {
  font-size: 12px;
  color: #4a6080;
}

.device-id-value {
  font-size: 12px;
  color: #8aaacf;
  font-family: monospace;
}

.tab-wrap {
  margin: 16px auto 0;
  width: 100%;
  max-width: 1240px;
  display: flex;
  background: #0d1726;
  border: 1px solid #1a2840;
  border-radius: 14px;
  padding: 5px;
  gap: 4px;
}

.tab-btn {
  flex: 1;
  height: 52px;
  border-radius: 10px;
  font-size: 15px;
  font-weight: 600;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  text-decoration: none;
}

.tab-btn.active {
  background: #2563eb;
  color: #ffffff;
}

.tab-btn.inactive {
  background: transparent;
  color: #4a6080;
}

.tab-btn.inactive:hover {
  color: #8aaacf;
}

@media (max-width: 900px) {
  .vault-shell {
    padding: 20px 16px 0;
  }

  .tab-btn {
    height: 44px;
    font-size: 28px;
  }
}
</style>
