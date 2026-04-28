<script setup lang="ts">
import { AppState } from '@/stores/app-state';
import { UserDataOutsiderStatus } from 'meta-secret-web-cli';
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useRouter } from 'vue-router';

const router = useRouter();
const jsAppState = AppState();

const signUpProcessing = ref(false);
const signUpCompleted = ref(false);
const isCleaning = ref(false);
const vaultName = ref('');
const hasSubmittedVaultName = ref(false);
const isCheckingVaultName = ref(false);

const progress = ref(0);
const progressInterval = ref<number | null>(null);

const outsiderStatus = computed<number | null>(() => {
  try {
    if (jsAppState.currState && jsAppState.isOutsider) {
      const vaultState = jsAppState.currState.as_vault();
      if (vaultState.is_outsider()) {
        return Number(vaultState.as_outsider().status);
      }
    }
    return null;
  } catch {
    return null;
  }
});

const isNonMember = computed(() => outsiderStatus.value === UserDataOutsiderStatus.NonMember);
const isPending = computed(() => outsiderStatus.value === UserDataOutsiderStatus.Pending);
const isDeclined = computed(() => outsiderStatus.value === UserDataOutsiderStatus.Declined);

const stopProgressSimulation = () => {
  if (progressInterval.value !== null) {
    clearInterval(progressInterval.value);
    progressInterval.value = null;
  }
};

const startProgressSimulation = () => {
  progress.value = 0;
  stopProgressSimulation();
  progressInterval.value = window.setInterval(() => {
    if (progress.value < 90) {
      progress.value += Math.random() * 8;
    }
  }, 200);
};

watch(
  () => signUpProcessing.value,
  (active) => {
    if (active) {
      startProgressSimulation();
      return;
    }
    stopProgressSimulation();
    progress.value = 0;
  },
  { immediate: true },
);

watch(
  () => signUpCompleted.value,
  (completed) => {
    if (completed && signUpProcessing.value) {
      stopProgressSimulation();
      progress.value = 100;
    }
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  stopProgressSimulation();
});

const updateVaultName = (event: Event) => {
  const input = event.target as HTMLInputElement;
  vaultName.value = input.value;
  hasSubmittedVaultName.value = false;
};

const generateUserCreds = async () => {
  if (signUpProcessing.value || isCheckingVaultName.value) return;
  if (!vaultName.value.trim()) return;
  hasSubmittedVaultName.value = true;
  isCheckingVaultName.value = true;
  // @ts-ignore - Method exists in Rust but TS definitions may be outdated
  try {
    await jsAppState.appManager.generate_user_creds(vaultName.value);
    await jsAppState.appStateInit();
  } catch (error) {
    hasSubmittedVaultName.value = false;
    throw error;
  } finally {
    isCheckingVaultName.value = false;
  }
};

const signUp = async () => {
  if (signUpProcessing.value) return;

  signUpProcessing.value = true;
  signUpCompleted.value = false;

  try {
    const newState = await jsAppState.appManager.sign_up();
    signUpCompleted.value = true;
    jsAppState.updateStateWith(newState);
  } catch {
    signUpProcessing.value = false;
    signUpCompleted.value = false;
  }
};

const cleanDatabase = async () => {
  if (isCleaning.value) return;

  isCleaning.value = true;
  try {
    await jsAppState.cleanDatabase();
    await jsAppState.appStateInit();
    hasSubmittedVaultName.value = false;
    vaultName.value = '';
    await router.push('/');
  } finally {
    isCleaning.value = false;
  }
};

const progressTitle = computed(() => {
  if (jsAppState.isOutsider) return 'Joining Vault...';
  if (jsAppState.isVaultNotExists) return 'Creating Vault...';
  return 'Processing...';
});

const progressMessage = computed(() => {
  if (jsAppState.isOutsider) {
    return "Please don't close this page. Your request to join the vault is being processed...";
  }
  if (jsAppState.isVaultNotExists) {
    return "Please don't close this page. Vault creation is in progress...";
  }
  return "Please don't close this page. Operation in progress...";
});
</script>

<template>
  <div class="setup-screen">
    <div class="setup-card">
      <label class="field-label">Enter vault name</label>
      <div class="input-row">
        <div class="input-wrap">
          <span class="at-char">@</span>
          <input
            class="input-field"
            placeholder="vault name"
            :value="vaultName"
            :disabled="signUpProcessing"
            @input="updateVaultName"
          />
        </div>
        <button class="btn-primary" :disabled="signUpProcessing || isCleaning || isCheckingVaultName || !vaultName.trim()" @click="generateUserCreds">
          <span v-if="isCheckingVaultName">Checking...</span>
          <span v-else>Set Vault Name</span>
        </button>
      </div>

      <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isVaultNotExists">
        <div class="status-row">
          <span class="status-text">Vault name is free!</span>
          <button class="btn-primary compact" :disabled="signUpProcessing" @click="signUp">Create</button>
        </div>
      </template>

      <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isOutsider && isNonMember">
        <div class="status-row split">
          <span class="status-text">Vault already exists, would you like to join?</span>
          <div class="btn-group">
            <button class="btn-secondary compact" :disabled="isCleaning || signUpProcessing" @click="cleanDatabase">
              <span v-if="isCleaning">Cleaning...</span>
              <span v-else>Reset</span>
            </button>
            <button class="btn-primary compact" :disabled="signUpProcessing || isCleaning" @click="signUp">Join</button>
          </div>
        </div>
      </template>

      <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isOutsider && isPending">
        <div class="status-block pending">
          <div class="status-title">Your request to join this vault is pending approval.</div>
          <button class="btn-secondary compact" :disabled="isCleaning || signUpProcessing" @click="cleanDatabase">
            <span v-if="isCleaning">Cleaning...</span>
            <span v-else>Reset & Start Over</span>
          </button>
        </div>
      </template>

      <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isOutsider && isDeclined">
        <div class="status-block declined">
          <div class="status-title">Your request to join this vault was declined.</div>
          <button class="btn-secondary compact" :disabled="isCleaning || signUpProcessing" @click="cleanDatabase">
            <span v-if="isCleaning">Cleaning...</span>
            <span v-else>Reset & Create New</span>
          </button>
        </div>
      </template>

      <template v-if="hasSubmittedVaultName && !isCheckingVaultName && jsAppState.isOutsider && outsiderStatus === null">
        <div class="status-block declined">
          <div class="status-title">Status is unknown. Please reset and try again.</div>
          <button class="btn-secondary compact" :disabled="isCleaning || signUpProcessing" @click="cleanDatabase">
            <span v-if="isCleaning">Cleaning...</span>
            <span v-else>Reset & Create New</span>
          </button>
        </div>
      </template>
    </div>

    <div v-if="signUpProcessing" class="progress-box">
      <div class="progress-title">{{ progressTitle }}</div>
      <p class="progress-text">{{ progressMessage }}</p>
      <div class="progress-track">
        <div class="progress-fill" :style="{ width: `${Math.min(progress, 100)}%` }"></div>
      </div>
      <p class="progress-percent">{{ Math.floor(progress) }}%</p>
    </div>

    <div class="learn-more-banner">
      <div>
        <div class="learn-title">New to Meta Secret?</div>
        <div class="learn-sub">Learn how Meta Secret works and how to use it effectively</div>
      </div>
      <button class="btn-primary compact" @click="router.push('/info')">Learn More</button>
    </div>
  </div>
</template>

<style scoped>
.setup-screen {
  min-height: calc(100vh - 60px);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 22px;
  padding: 24px;
}

.setup-card {
  width: 100%;
  max-width: 440px;
  background: #0d1726;
  border: 1px solid #1a2840;
  border-radius: 20px;
  padding: 24px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.field-label {
  font-size: 14px;
  color: #4a6080;
  font-weight: 500;
}

.input-row {
  display: flex;
  gap: 10px;
}

.input-wrap {
  position: relative;
  flex: 1;
}

.at-char {
  position: absolute;
  left: 14px;
  top: 50%;
  transform: translateY(-50%);
  color: #3a5070;
  font-size: 16px;
}

.input-field {
  width: 100%;
  height: 46px;
  border-radius: 12px;
  border: 1px solid #1e3050;
  background: #111e30;
  color: #ffffff;
  font-size: 15px;
  padding: 0 14px 0 34px;
  outline: none;
}

.input-field::placeholder {
  color: #3a5070;
}

.input-field:focus {
  border-color: #2563eb;
}

.status-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  background: #080f1c;
  border-radius: 12px;
  border: 1px solid #1a2840;
  padding: 14px 16px;
}

.status-row.split {
  align-items: center;
}

.status-text {
  color: #8aaacf;
  font-size: 14px;
  line-height: 1.4;
}

.btn-group {
  display: flex;
  gap: 8px;
}

.status-block {
  background: #080f1c;
  border: 1px solid #1a2840;
  border-radius: 12px;
  padding: 14px 16px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.status-block.pending .status-title {
  color: #facc15;
}

.status-block.declined .status-title {
  color: #f87171;
}

.status-title {
  font-size: 14px;
}

.btn-primary {
  background: #2563eb;
  color: #ffffff;
  border: none;
  border-radius: 12px;
  height: 46px;
  padding: 0 20px;
  font-size: 14px;
  font-weight: 700;
  cursor: pointer;
  white-space: nowrap;
}

.btn-primary:hover:not(:disabled) {
  opacity: 0.9;
}

.btn-secondary {
  background: #111e30;
  color: #8aaacf;
  border: 1px solid #1e3050;
  border-radius: 12px;
  height: 46px;
  padding: 0 16px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
}

.btn-secondary:hover:not(:disabled) {
  border-color: #2563eb44;
  color: #ffffff;
}

.compact {
  height: 40px;
  padding: 0 16px;
}

.btn-primary:disabled,
.btn-secondary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.progress-box {
  width: 100%;
  max-width: 440px;
  background: #111c2c;
  border: 1px solid #2a3a52;
  border-radius: 14px;
  padding: 16px;
}

.progress-title {
  color: #e6b44a;
  font-size: 15px;
  font-weight: 700;
  margin-bottom: 6px;
}

.progress-text {
  color: #8aaacf;
  font-size: 13px;
  margin-bottom: 10px;
}

.progress-track {
  height: 8px;
  background: #1a2840;
  border-radius: 999px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: #e6b44a;
  transition: width 0.2s;
}

.progress-percent {
  margin-top: 6px;
  color: #4a6080;
  font-size: 12px;
  text-align: center;
}

.learn-more-banner {
  width: 100%;
  max-width: 440px;
  background: linear-gradient(135deg, #1a2e5a 0%, #1a3050 100%);
  border: 1px solid #1e3a6a;
  border-radius: 14px;
  padding: 16px 20px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.learn-title {
  font-size: 14px;
  font-weight: 700;
  color: #ffffff;
  margin-bottom: 3px;
}

.learn-sub {
  font-size: 12px;
  color: #4a7aae;
}

@media (max-width: 700px) {
  .setup-card,
  .learn-more-banner,
  .progress-box {
    max-width: 100%;
  }

  .input-row,
  .status-row,
  .status-block,
  .learn-more-banner {
    flex-direction: column;
    align-items: stretch;
  }

  .btn-group {
    width: 100%;
  }

  .btn-group .btn-primary,
  .btn-group .btn-secondary,
  .input-row .btn-primary,
  .status-row .btn-primary,
  .status-block .btn-secondary,
  .learn-more-banner .btn-primary {
    width: 100%;
  }
}
</style>
