<script setup lang="ts">
import { computed, ref } from 'vue';
import { MetaPasswordId } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { vaultSecrets } from '@/locales/en';
import AddSecretForm from './AddSecretForm.vue';

type RevealModalState = 'closed' | 'waiting' | 'revealedText' | 'revealedSeed';

const appState = AppState();
const appManager = appState.appManager as any;

const showAddForm = ref(false);
const passwords = computed(() => appState.passwords);

const activeSecret = ref<any | null>(null);
const activeSecretId = ref<string | null>(null);
const revealedSecret = ref('');
const revealedWords = ref<string[]>([]);
const revealModalState = ref<RevealModalState>('closed');
const flowError = ref<string | null>(null);
const flowInProgressId = ref<string | null>(null);
const copyInProgress = ref(false);
const copySucceeded = ref(false);
const flowToken = ref(0);

const FLOW_MAX_ATTEMPTS = 15;
const FLOW_POLL_DELAY_MS = 800;

const toggleAddForm = () => {
  showAddForm.value = !showAddForm.value;
};

const handleSecretAdded = () => {
  showAddForm.value = false;
};

const isRecovered = (metaPassId: MetaPasswordId) => {
  const maybeCompletedClaim = (appState.currState as any).as_vault?.()?.as_member?.()?.find_recovery_claim(metaPassId);
  return maybeCompletedClaim !== undefined;
};

const isFlowTokenActive = (token: number) => token === flowToken.value;

const sleep = (ms: number) => new Promise((resolve) => {
  setTimeout(resolve, ms);
});

const clearRevealData = () => {
  revealedSecret.value = '';
  revealedWords.value = [];
  copyInProgress.value = false;
  copySucceeded.value = false;
};

const closeAllSecretModals = () => {
  flowToken.value += 1;
  revealModalState.value = 'closed';
  activeSecret.value = null;
  activeSecretId.value = null;
  flowInProgressId.value = null;
  clearRevealData();
};

const parseSecretType = (secretValue: string) => {
  const words = secretValue.trim().split(/\s+/).filter(Boolean);
  if (words.length === 12 || words.length === 24) {
    return { type: 'seed' as const, words };
  }
  return { type: 'text' as const, words: [] };
};

const openRevealedModal = (secretValue: string) => {
  const parsed = parseSecretType(secretValue);
  if (parsed.type === 'seed') {
    revealedWords.value = parsed.words;
    revealedSecret.value = '';
    revealModalState.value = 'revealedSeed';
    return;
  }
  revealedSecret.value = secretValue;
  revealedWords.value = [];
  revealModalState.value = 'revealedText';
};

const waitForRecoveredClaim = async (metaPassId: MetaPasswordId, token: number) => {
  if (isRecovered(metaPassId)) return true;

  for (let i = 0; i < FLOW_MAX_ATTEMPTS; i += 1) {
    if (!isFlowTokenActive(token) || revealModalState.value !== 'waiting') return false;
    await sleep(FLOW_POLL_DELAY_MS);
    if (!isFlowTokenActive(token) || revealModalState.value !== 'waiting') return false;
    await appState.updateState();
    if (isRecovered(metaPassId)) return true;
  }

  throw new Error(vaultSecrets.errorRecoveryTimeout);
};

const startRevealFlow = async (secret: any) => {
  const secretId = secret.id_str();

  if (flowInProgressId.value && flowInProgressId.value !== secretId) return;
  if (flowInProgressId.value === secretId) return;

  const token = flowToken.value + 1;
  flowToken.value = token;
  flowError.value = null;
  activeSecret.value = secret;
  activeSecretId.value = secretId;
  clearRevealData();
  revealModalState.value = 'waiting';
  flowInProgressId.value = secretId;

  try {
    await appManager.recover_js(secret);
    if (!isFlowTokenActive(token)) return;
    await appState.updateState();
    if (!isFlowTokenActive(token)) return;

    await waitForRecoveredClaim(secret, token);
    if (!isFlowTokenActive(token)) return;

    const secretText = await appManager.show_recovered(secret);
    if (!isFlowTokenActive(token)) return;

    openRevealedModal(secretText);
  } catch (e) {
    console.error('Secrets reveal flow failed', e);
    if (!isFlowTokenActive(token)) return;
    flowError.value = e instanceof Error ? e.message : vaultSecrets.errorShowRecovered;
    revealModalState.value = 'closed';
  } finally {
    if (isFlowTokenActive(token)) {
      flowInProgressId.value = null;
    }
  }
};

const copyRevealedValue = async () => {
  if (copyInProgress.value) return;
  const valueToCopy = revealModalState.value === 'revealedSeed'
    ? revealedWords.value.join(' ')
    : revealedSecret.value;
  if (!valueToCopy) return;

  copyInProgress.value = true;
  try {
    await navigator.clipboard.writeText(valueToCopy);
    copySucceeded.value = true;
    setTimeout(() => {
      copySucceeded.value = false;
    }, 2000);
  } catch (e) {
    console.error('Failed to copy revealed secret', e);
    flowError.value = vaultSecrets.errorCopySecret;
  } finally {
    copyInProgress.value = false;
  }
};

const waitingDeviceCount = computed(() => {
  const maybeVaultData = (appState.currState as any).as_vault?.()?.as_member?.()?.vault_data?.();
  if (!maybeVaultData || typeof maybeVaultData.users !== 'function') return 1;
  const count = maybeVaultData.users().length;
  return count > 0 ? count : 1;
});

const currentDeviceCount = computed(() => {
  const maybeVaultData = (appState.currState as any).as_vault?.()?.as_member?.()?.vault_data?.();
  if (!maybeVaultData || typeof maybeVaultData.users !== 'function') return 0;
  return maybeVaultData.users().length;
});

const requiredDevicesToSafety = computed(() => 3 - currentDeviceCount.value);

const shouldShowDevicesWarning = computed(() => requiredDevicesToSafety.value > 0);
</script>

<template>
  <div :class="$style.mainContent">
    <div :class="$style.pageWide">
      <div v-if="shouldShowDevicesWarning" :class="$style.warningBanner">
        <span :class="$style.warningIcon">⚠</span>
        <span>
          {{ vaultSecrets.warningPrefix }} {{ requiredDevicesToSafety }} {{ vaultSecrets.warningMiddle }}
        </span>
      </div>

      <div :class="$style.secretsCard">
        <div :class="$style.secretsHeader">
          <div>
            <h3 :class="$style.secretsTitle">{{ vaultSecrets.title }}</h3>
            <div :class="$style.sectionSub">{{ vaultSecrets.subtitle }}</div>
          </div>
          <button :class="$style.addSecretButton" @click="toggleAddForm">{{ vaultSecrets.addSecret }}</button>
        </div>

        <div v-if="passwords.length === 0" :class="$style.emptyState">{{ vaultSecrets.emptyState }}</div>

        <div v-else>
          <div v-if="flowError" :class="$style.inlineError" role="alert">{{ flowError }}</div>
          <ul :class="$style.secretsList">
            <li v-for="secret in passwords" :key="secret.id_str()" :class="$style.secretRow">
              <div :class="$style.secretName">{{ secret.name }}</div>
              <button
                :class="$style.showButton"
                :disabled="flowInProgressId !== null"
                @click="startRevealFlow(secret)"
              >
                {{ flowInProgressId === secret.id_str() ? vaultSecrets.showLoading : vaultSecrets.show }}
              </button>
            </li>
          </ul>
        </div>
      </div>
    </div>

    <AddSecretForm :show="showAddForm" @added="handleSecretAdded" @close="toggleAddForm" />
  </div>

  <div v-if="revealModalState !== 'closed'" :class="$style.modalOverlay" @click="closeAllSecretModals">
    <div :class="$style.modalBox" @click.stop>
      <div :class="$style.modalHeader">
        <span>{{ activeSecret?.name || '' }}</span>
        <button :class="$style.closeXButton" @click="closeAllSecretModals">×</button>
      </div>

      <div v-if="revealModalState === 'waiting'" :class="$style.encryptedCard">
        <div :class="$style.lockCircle">
          <svg width="30" height="30" viewBox="0 0 24 24" fill="none">
            <rect x="5" y="11" width="14" height="11" rx="3" stroke="#3b7eff" stroke-width="2"/>
            <path d="M8 11V7a4 4 0 0 1 8 0v4" stroke="#3b7eff" stroke-width="2" stroke-linecap="round"/>
            <circle cx="12" cy="16" r="1.5" fill="#3b7eff"/>
          </svg>
        </div>
        <div :class="$style.encryptedTitle">{{ vaultSecrets.waitingTitle }}</div>
        <div :class="$style.encryptedSubtitle">{{ vaultSecrets.waitingSubtitle }}</div>
        <div :class="$style.placeholderLines">
          <div :class="$style.placeholderLineOne"></div>
          <div :class="$style.placeholderLineTwo"></div>
          <div :class="$style.placeholderLineThree"></div>
          <div :class="$style.placeholderLineFour"></div>
        </div>
        <div :class="$style.devicesCount">
          {{ waitingDeviceCount }}
          {{ waitingDeviceCount === 1 ? vaultSecrets.waitingDevicesSuffix : vaultSecrets.waitingDevicesSuffixPlural }}
        </div>
      </div>

      <template v-else-if="revealModalState === 'revealedText'">
        <div :class="$style.revealedSecretBox">
          <span :class="$style.revealedSecretLabel">{{ vaultSecrets.secretLabel }}</span>
          <span :class="$style.revealedSecretValue">{{ revealedSecret }}</span>
        </div>
      </template>

      <template v-else-if="revealModalState === 'revealedSeed'">
        <div :class="$style.seedGrid">
          <div v-for="(word, index) in revealedWords" :key="`${index}-${word}`" :class="$style.seedCell">
            <span :class="$style.seedIndex">{{ index + 1 }}</span>
            <span :class="$style.seedWord">{{ word }}</span>
          </div>
        </div>
      </template>

      <div v-if="revealModalState !== 'waiting'" :class="$style.modalActions">
        <button :class="$style.btnSecondary" @click="closeAllSecretModals">{{ vaultSecrets.close }}</button>
        <button :class="$style.btnPrimary" :disabled="copyInProgress" @click="copyRevealedValue">
          {{
            copySucceeded
              ? vaultSecrets.copied
              : (revealModalState === 'revealedSeed' ? vaultSecrets.copyPhrase : vaultSecrets.copySecret)
          }}
        </button>
      </div>
    </div>
  </div>
</template>

<style module>
.mainContent {
  padding: 48px 24px;
  display: flex;
  justify-content: center;
}

.pageWide {
  width: 100%;
  max-width: 1240px;
}

.warningBanner {
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

.warningIcon {
  color: #e6b44a;
}

.secretsCard {
  background: #0d1726;
  border: 1px solid #1a2840;
  border-radius: 16px;
  overflow: hidden;
}

.secretsHeader {
  padding: 18px 20px;
  border-bottom: 1px solid #1a2840;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.secretsTitle {
  font-size: 17px;
  line-height: 1.2;
  font-weight: 700;
  color: #ffffff;
}

.sectionSub {
  margin-top: 2px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: #3a5070;
}

.addSecretButton {
  background: #2563eb;
  color: #ffffff;
  border: none;
  border-radius: 12px;
  height: 46px;
  padding: 0 24px;
  font-size: 15px;
  font-weight: 700;
  cursor: pointer;
}

.emptyState {
  padding: 30px;
  color: #8aaacf;
  font-size: 14px;
  text-align: center;
}

.inlineError {
  margin: 14px 20px 0;
  background: #3a1820;
  border: 1px solid #a3213d;
  color: #fca5a5;
  border-radius: 10px;
  padding: 10px 12px;
  font-size: 13px;
}

.secretsList {
  list-style: none;
  margin: 0;
  padding: 0;
}

.secretRow {
  padding: 16px 20px;
  border-bottom: 1px solid #1a2840;
  display: flex;
  align-items: center;
}

.secretRow:last-child {
  border-bottom: none;
}

.secretName {
  color: #ffffff;
  font-size: 15px;
  font-weight: 700;
  flex: 1;
}

.showButton {
  background: transparent;
  border: 1px solid #1a2840;
  color: #8aaacf;
  border-radius: 10px;
  height: 38px;
  min-width: 72px;
  padding: 0 16px;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
}

.showButton:hover:not(:disabled) {
  border-color: #2563eb55;
  color: #ffffff;
}

.showButton:disabled {
  opacity: 0.65;
  cursor: wait;
}

.modalOverlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.68);
  backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 230;
  padding: 24px;
}

.modalBox {
  width: 100%;
  max-width: 760px;
  border-radius: 22px;
  border: 1px solid #1e3050;
  background: #0d1726;
  box-shadow: 0 32px 80px rgba(0, 0, 0, 0.6);
  padding: 24px;
}

.modalHeader {
  display: flex;
  justify-content: space-between;
  align-items: center;
  color: #ffffff;
  font-size: 17px;
  font-weight: 800;
  margin-bottom: 14px;
}

.closeXButton {
  width: 34px;
  height: 34px;
  border-radius: 10px;
  border: 1px solid transparent;
  background: transparent;
  color: #4a6080;
  font-size: 24px;
  cursor: pointer;
}

.closeXButton:hover {
  border-color: #1e3050;
}

.encryptedCard {
  background: #080f1c;
  border: 1px solid #1a2840;
  border-radius: 14px;
  padding: 24px 20px;
  display: flex;
  flex-direction: column;
  align-items: center;
}

.lockCircle {
  width: 64px;
  height: 64px;
  border-radius: 50%;
  background: #1a2e4a;
  display: flex;
  align-items: center;
  justify-content: center;
}

.encryptedTitle {
  margin-top: 10px;
  color: #ffffff;
  font-size: 17px;
  font-weight: 700;
}

.encryptedSubtitle {
  margin-top: 6px;
  color: #3a5070;
  font-size: 14px;
  text-align: center;
  line-height: 1.4;
}

.placeholderLines {
  margin-top: 12px;
  width: 100%;
  max-width: 500px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.placeholderLineOne,
.placeholderLineTwo,
.placeholderLineThree,
.placeholderLineFour {
  height: 10px;
  border-radius: 6px;
  background: #1a2840;
}

.placeholderLineOne {
  width: 88%;
}

.placeholderLineTwo {
  width: 66%;
}

.placeholderLineThree {
  width: 82%;
}

.placeholderLineFour {
  width: 59%;
}

.devicesCount {
  margin-top: 14px;
  color: #4a6080;
  font-size: 14px;
}

.revealedSecretBox {
  background: #080f1c;
  border: 1px solid #1a2840;
  border-radius: 14px;
  padding: 18px 22px;
  display: flex;
  align-items: center;
  gap: 14px;
  color: #ffffff;
  font-size: 38px;
  font-weight: 700;
  overflow-wrap: anywhere;
}

.revealedSecretLabel {
  color: #4a6080;
  font-family: ui-monospace, Menlo, Monaco, 'Cascadia Mono', 'Segoe UI Mono', 'Roboto Mono', 'Oxygen Mono', 'Ubuntu Monospace', 'Source Code Pro', 'Fira Mono', 'Droid Sans Mono', 'Courier New', monospace;
  font-size: 44px;
  font-weight: 700;
}

.revealedSecretValue {
  color: #91bdff;
  word-break: break-word;
}

.seedGrid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}

.seedCell {
  border: 1px solid #1a2840;
  border-radius: 12px;
  background: #111e30;
  min-height: 46px;
  padding: 8px 10px;
  display: flex;
  align-items: center;
  gap: 10px;
}

.seedIndex {
  color: #3a5070;
  font-size: 12px;
  font-weight: 700;
  min-width: 20px;
}

.seedWord {
  color: #ffffff;
  font-size: 32px;
  font-weight: 600;
}

.modalActions {
  margin-top: 18px;
  display: flex;
  justify-content: flex-end;
  gap: 12px;
}

.btnSecondary,
.btnPrimary {
  border-radius: 18px;
  height: 86px;
  font-size: 44px;
  font-weight: 700;
  border: none;
  cursor: pointer;
  padding: 0 36px;
}

.btnSecondary {
  background: #111e30;
  border: 1px solid #1e3050;
  color: #8aaacf;
}

.btnPrimary {
  background: #2563eb;
  color: #ffffff;
}

.btnPrimary:disabled {
  opacity: 0.65;
  cursor: wait;
}
</style>
