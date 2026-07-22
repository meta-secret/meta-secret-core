<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ClaimId, MetaPasswordId, WasmApplicationManager } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';
import { vaultSecrets } from '@/locales/en';
import AddSecretForm from './AddSecretForm.vue';
import { getAppManager, getMemberVaultData, getMemberVaultState } from '@/utils/wasmBridge';
import { shouldShowRecoveryRequestIcon } from '@/utils/recoveryRequest';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Lock } from 'lucide-vue-next';

type RevealModalState = 'closed' | 'waiting' | 'revealedText' | 'revealedSeed';
type RecoveryAction = 'approve' | 'decline';
type RecoveryAwareMemberState = ReturnType<typeof getMemberVaultState> & {
  find_pending_incoming_recovery_claim?: (metaPassId: MetaPasswordId) => ClaimId | undefined;
  recovery_client_status?: (metaPassId: MetaPasswordId) => string | undefined;
};
type RecoveryAwareApplicationManager = WasmApplicationManager & {
  accept_recover?: (claimId: ClaimId) => Promise<void>;
  decline_recover?: (claimId: ClaimId) => Promise<void>;
};

const appState = AppState();
const appManager = getAppManager();
const authStore = useAuthStore();

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
const recoveryDialogOpen = ref(false);
const recoveryDialogSecret = ref<MetaPasswordId | null>(null);
const recoveryDialogClaim = ref<ClaimId | null>(null);
const recoveryActionInProgress = ref<RecoveryAction | null>(null);

const isRecovered = (metaPassId: MetaPasswordId) => {
  const claim = getMemberVaultState(appState.currState)?.find_recovery_claim(metaPassId);
  return claim !== undefined;
};

const getPendingIncomingRecoveryClaim = (metaPassId: MetaPasswordId) => {
  const memberState = getMemberVaultState(appState.currState) as RecoveryAwareMemberState | undefined;
  if (!memberState || typeof memberState.find_pending_incoming_recovery_claim !== 'function') return undefined;
  return memberState.find_pending_incoming_recovery_claim(metaPassId);
};

const hasPendingIncomingRecoveryRequest = (metaPassId: MetaPasswordId) =>
  shouldShowRecoveryRequestIcon(getPendingIncomingRecoveryClaim(metaPassId));

const getRecoveryClientStatus = (metaPassId: MetaPasswordId): string | undefined => {
  const memberState = getMemberVaultState(appState.currState) as RecoveryAwareMemberState | undefined;
  if (!memberState || typeof memberState.recovery_client_status !== 'function') return undefined;
  return memberState.recovery_client_status(metaPassId);
};

const isFlowTokenActive = (token: number) => token === flowToken.value;
const getVaultDeviceCount = () => {
  const data = getMemberVaultData(appState.currState);
  if (!data || typeof data.users !== 'function') return 1;
  return data.users().length || 1;
};
const actionButtonLabel = computed(() => (getVaultDeviceCount() <= 2 ? vaultSecrets.show : vaultSecrets.recover));

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

const openRecoveryDialog = (secret: MetaPasswordId) => {
  const claim = getPendingIncomingRecoveryClaim(secret);
  if (!claim || recoveryActionInProgress.value) return;
  recoveryDialogSecret.value = secret;
  recoveryDialogClaim.value = claim;
  recoveryDialogOpen.value = true;
};

const resetRecoveryDialog = () => {
  recoveryDialogOpen.value = false;
  recoveryDialogSecret.value = null;
  recoveryDialogClaim.value = null;
};

const closeRecoveryDialog = () => {
  if (recoveryActionInProgress.value) return;
  resetRecoveryDialog();
};

const submitRecoveryResponse = async (action: RecoveryAction) => {
  console.log('[Recovery] submitRecoveryResponse called', { action, claim: recoveryDialogClaim.value, inProgress: recoveryActionInProgress.value });
  if (recoveryActionInProgress.value || !recoveryDialogClaim.value) {
    console.warn('[Recovery] early return — inProgress:', recoveryActionInProgress.value, 'claim:', recoveryDialogClaim.value);
    return;
  }
  recoveryActionInProgress.value = action;
  flowError.value = null;

  try {
    console.log('[Recovery] calling authenticateWithPasskey...');
    const authenticated = await authStore.authenticateWithPasskey();
    console.log('[Recovery] authenticateWithPasskey result:', authenticated);
    if (!authenticated) throw new Error(vaultSecrets.recoveryRequestAuthError);

    const recoveryAppManager = appManager as RecoveryAwareApplicationManager;
    if (action === 'approve') {
      console.log('[Recovery] calling accept_recover, claimId:', recoveryDialogClaim.value);
      if (typeof recoveryAppManager.accept_recover !== 'function')
        throw new Error(vaultSecrets.recoveryRequestSubmitError);
      await recoveryAppManager.accept_recover(recoveryDialogClaim.value);
      console.log('[Recovery] accept_recover done');
    } else {
      console.log('[Recovery] calling decline_recover, claimId:', recoveryDialogClaim.value);
      if (typeof recoveryAppManager.decline_recover !== 'function')
        throw new Error(vaultSecrets.recoveryRequestSubmitError);
      await recoveryAppManager.decline_recover(recoveryDialogClaim.value);
      console.log('[Recovery] decline_recover done');
    }

    await appState.updateState();
    resetRecoveryDialog();
  } catch (e) {
    console.error('[Recovery] error:', e);
    flowError.value = e instanceof Error && e.message ? e.message : vaultSecrets.recoveryRequestSubmitError;
  } finally {
    recoveryActionInProgress.value = null;
  }
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
  const resolveFromCurrentState = () => {
    if (!isFlowTokenActive(token) || revealModalState.value !== 'waiting') return false;
    if (isRecovered(metaPassId)) return true;

    const status = getRecoveryClientStatus(metaPassId);
    if (status === 'accepted' || status === 'done') return true;
    if (status === 'declined') throw new Error(vaultSecrets.errorRecoveryDeclined);
    return undefined;
  };

  const immediateResult = resolveFromCurrentState();
  if (immediateResult !== undefined) return immediateResult;

  return new Promise<boolean>((resolve, reject) => {
    const stop = watch(
      () => [appState.currState, flowToken.value, revealModalState.value] as const,
      () => {
        try {
          const result = resolveFromCurrentState();
          if (result === undefined) return;
          stop();
          resolve(result);
        } catch (error) {
          stop();
          reject(error);
        }
      },
    );
  });
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
  flowInProgressId.value = secretId;

  try {
    await appState.updateState();
    if (!isFlowTokenActive(token)) return;
    const deviceCount = getVaultDeviceCount();

    if (deviceCount <= 2) {
      const secretText = await appManager.show_recovered(secret);
      if (!isFlowTokenActive(token)) return;
      openRevealedModal(secretText);
      return;
    }

    revealModalState.value = 'waiting';
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
          <li
            v-for="secret in passwords"
            :key="secret.id_str()"
            class="flex items-center justify-between px-5 py-4 transition-colors"
            :class="hasPendingIncomingRecoveryRequest(secret) && 'cursor-pointer hover:bg-muted/40'"
            :role="hasPendingIncomingRecoveryRequest(secret) ? 'button' : undefined"
            :tabindex="hasPendingIncomingRecoveryRequest(secret) ? 0 : undefined"
            @click="hasPendingIncomingRecoveryRequest(secret) && openRecoveryDialog(secret)"
            @keydown.enter.prevent="hasPendingIncomingRecoveryRequest(secret) && openRecoveryDialog(secret)"
            @keydown.space.prevent="hasPendingIncomingRecoveryRequest(secret) && openRecoveryDialog(secret)"
          >
            <span class="font-semibold">{{ secret.name }}</span>
            <div class="flex items-center gap-4">
              <button
                v-if="hasPendingIncomingRecoveryRequest(secret)"
                type="button"
                class="flex h-12 w-12 items-center justify-center rounded-full border border-transparent transition-colors hover:border-primary/40 hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                :aria-label="vaultSecrets.recoveryRequestTitle"
                @click.stop="openRecoveryDialog(secret)"
              >
                <img src="/approve_request.png" alt="" class="h-10 w-10 rounded-full object-cover" />
              </button>
              <Button
                variant="outline"
                size="sm"
                :disabled="flowInProgressId !== null"
                @click.stop="startRevealFlow(secret)"
              >
                {{ flowInProgressId === secret.id_str() ? vaultSecrets.showLoading : actionButtonLabel }}
              </Button>
            </div>
          </li>
        </ul>
      </CardContent>
    </Card>

    <AddSecretForm :show="showAddForm" @added="showAddForm = false" @close="showAddForm = false" />
  </div>

  <AlertDialog :open="recoveryDialogOpen" @update:open="(open) => !open && closeRecoveryDialog()">
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>{{ vaultSecrets.recoveryRequestTitle }}</AlertDialogTitle>
        <AlertDialogDescription>
          {{ recoveryDialogSecret?.name || '' }}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <Button
          variant="outline"
          :disabled="recoveryActionInProgress !== null"
          @click="submitRecoveryResponse('decline')"
        >
          {{ recoveryActionInProgress === 'decline' ? vaultSecrets.showLoading : vaultSecrets.recoveryRequestDecline }}
        </Button>
        <Button
          :disabled="recoveryActionInProgress !== null"
          @click="submitRecoveryResponse('approve')"
        >
          {{ recoveryActionInProgress === 'approve' ? vaultSecrets.showLoading : vaultSecrets.recoveryRequestApprove }}
        </Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>

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
