<script setup lang="ts">
import { computed, ref } from 'vue';
import { MetaPasswordId } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { vaultSecrets } from '@/locales/en';
import AddSecretForm from './AddSecretForm.vue';
import { getAppManager, getMemberVaultData, getMemberVaultState } from '@/utils/wasmBridge';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Lock } from 'lucide-vue-next';

type RevealModalState = 'closed' | 'waiting' | 'revealedText' | 'revealedSeed';

const appState = AppState();
const appManager = getAppManager();

const showAddForm = ref(false);
const passwords = computed(() => appState.passwords);

const activeSecret = ref<MetaPasswordId | null>(null);
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

const isRecovered = (metaPassId: MetaPasswordId) => {
  const claim = getMemberVaultState(appState.currState)?.find_recovery_claim(metaPassId);
  return claim !== undefined;
};

const isFlowTokenActive = (token: number) => token === flowToken.value;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

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
  if (words.length === 12 || words.length === 24) return { type: 'seed' as const, words };
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
  for (let i = 0; i < FLOW_MAX_ATTEMPTS; i++) {
    if (!isFlowTokenActive(token) || revealModalState.value !== 'waiting') return false;
    await sleep(FLOW_POLL_DELAY_MS);
    if (!isFlowTokenActive(token) || revealModalState.value !== 'waiting') return false;
    await appState.updateState();
    if (isRecovered(metaPassId)) return true;
  }
  throw new Error(vaultSecrets.errorRecoveryTimeout);
};

const startRevealFlow = async (secret: MetaPasswordId) => {
  const secretId = secret.id_str();
  if (flowInProgressId.value === secretId || (flowInProgressId.value && flowInProgressId.value !== secretId)) return;

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
    if (!isFlowTokenActive(token)) return;
    flowError.value = e instanceof Error ? e.message : vaultSecrets.errorShowRecovered;
    revealModalState.value = 'closed';
  } finally {
    if (isFlowTokenActive(token)) flowInProgressId.value = null;
  }
};

const copyRevealedValue = async () => {
  if (copyInProgress.value) return;
  const value = revealModalState.value === 'revealedSeed' ? revealedWords.value.join(' ') : revealedSecret.value;
  if (!value) return;
  copyInProgress.value = true;
  try {
    await navigator.clipboard.writeText(value);
    copySucceeded.value = true;
    setTimeout(() => {
      copySucceeded.value = false;
    }, 2000);
  } catch {
    flowError.value = vaultSecrets.errorCopySecret;
  } finally {
    copyInProgress.value = false;
  }
};

const waitingDeviceCount = computed(() => {
  const data = getMemberVaultData(appState.currState);
  if (!data || typeof data.users !== 'function') return 1;
  const n = data.users().length;
  return n > 0 ? n : 1;
});

const requiredDevicesToSafety = computed(() => {
  const data = getMemberVaultData(appState.currState);
  const n = data && typeof data.users === 'function' ? data.users().length : 0;
  return 3 - n;
});

const shouldShowDevicesWarning = computed(() => requiredDevicesToSafety.value > 0);
const revealModalOpen = computed(() => revealModalState.value !== 'closed');
</script>

<template>
  <div class="py-5">
    <Alert v-if="shouldShowDevicesWarning" class="mb-4">
      <AlertDescription>
        ⚠ {{ vaultSecrets.warningPrefix }} {{ requiredDevicesToSafety }} {{ vaultSecrets.warningMiddle }}
      </AlertDescription>
    </Alert>

    <Card>
      <CardHeader class="flex flex-row items-center justify-between border-b pb-4">
        <CardTitle class="text-base">{{ vaultSecrets.title }}</CardTitle>
        <Button size="sm" @click="showAddForm = true">{{ vaultSecrets.addSecret }}</Button>
      </CardHeader>

      <CardContent class="p-0">
        <div v-if="flowError" class="p-4">
          <Alert variant="destructive">
            <AlertDescription>{{ flowError }}</AlertDescription>
          </Alert>
        </div>

        <p v-if="passwords.length === 0" class="py-8 text-center text-sm text-muted-foreground">
          {{ vaultSecrets.emptyState }}
        </p>

        <ul v-else class="divide-y">
          <li v-for="secret in passwords" :key="secret.id_str()" class="flex items-center justify-between px-5 py-4">
            <span class="font-semibold">{{ secret.name }}</span>
            <Button variant="outline" size="sm" :disabled="flowInProgressId !== null" @click="startRevealFlow(secret)">
              {{ flowInProgressId === secret.id_str() ? vaultSecrets.showLoading : vaultSecrets.show }}
            </Button>
          </li>
        </ul>
      </CardContent>
    </Card>

    <AddSecretForm :show="showAddForm" @added="showAddForm = false" @close="showAddForm = false" />
  </div>

  <!-- Reveal modal -->
  <Dialog
    :open="revealModalOpen"
    @update:open="
      (v) => {
        if (!v) closeAllSecretModals();
      }
    "
  >
    <DialogContent class="max-w-2xl">
      <DialogHeader>
        <DialogTitle>{{ activeSecret?.name || '' }}</DialogTitle>
      </DialogHeader>

      <!-- Waiting state -->
      <div
        v-if="revealModalState === 'waiting'"
        class="flex flex-col items-center gap-4 rounded-lg border bg-muted/30 p-6"
      >
        <div class="flex h-16 w-16 items-center justify-center rounded-full bg-primary/10">
          <Lock class="h-7 w-7 text-primary" />
        </div>
        <p class="text-base font-semibold">{{ vaultSecrets.waitingTitle }}</p>
        <p class="text-xs text-muted-foreground">{{ vaultSecrets.waitingSubtitle }}</p>
        <div class="w-full max-w-sm space-y-2">
          <Skeleton class="h-2.5 w-[88%]" />
          <Skeleton class="h-2.5 w-[66%]" />
          <Skeleton class="h-2.5 w-[82%]" />
          <Skeleton class="h-2.5 w-[59%]" />
        </div>
        <p class="text-xs text-muted-foreground">
          {{ waitingDeviceCount }}
          {{ waitingDeviceCount === 1 ? vaultSecrets.waitingDevicesSuffix : vaultSecrets.waitingDevicesSuffixPlural }}
        </p>
      </div>

      <!-- Revealed text -->
      <template v-else-if="revealModalState === 'revealedText'">
        <div class="flex items-center gap-3 rounded-lg border bg-muted/30 px-4 py-3">
          <span class="font-mono text-sm text-muted-foreground">{{ vaultSecrets.secretLabel }}</span>
          <span class="break-all font-semibold text-primary">{{ revealedSecret }}</span>
        </div>
      </template>

      <!-- Revealed seed -->
      <template v-else-if="revealModalState === 'revealedSeed'">
        <div class="grid grid-cols-3 gap-2">
          <div
            v-for="(word, index) in revealedWords"
            :key="`${index}-${word}`"
            class="flex items-center gap-2 rounded-lg border bg-muted/30 px-3 py-2"
          >
            <span class="min-w-[1.5rem] text-xs font-bold text-muted-foreground">{{ index + 1 }}</span>
            <span class="font-semibold">{{ word }}</span>
          </div>
        </div>
      </template>

      <DialogFooter v-if="revealModalState !== 'waiting'">
        <Button variant="outline" @click="closeAllSecretModals">{{ vaultSecrets.close }}</Button>
        <Button :disabled="copyInProgress" @click="copyRevealedValue">
          {{
            copySucceeded
              ? vaultSecrets.copied
              : revealModalState === 'revealedSeed'
                ? vaultSecrets.copyPhrase
                : vaultSecrets.copySecret
          }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
