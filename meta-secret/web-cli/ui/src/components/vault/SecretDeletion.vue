<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { MetaPasswordId, WasmApplicationManager } from 'meta-secret-web-cli';
import { AppState } from '@/stores/app-state';
import { useAuthStore } from '@/stores/auth';
import { getMemberVaultState } from '@/utils/wasmBridge';
import { Button } from '@/components/ui/button';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';

type DeletionState = {
  secretId: string;
  requestId: string | null;
  requiresApproval: boolean;
  clientStatus: 'Pending' | 'NeedApprove' | 'Done' | 'Declined' | null;
};
const props = defineProps<{ secret: MetaPasswordId }>();
const appState = AppState();
const auth = useAuthStore();
const confirmOpen = ref(false);
const incomingOpen = ref(false);
const busy = ref(false);
const error = ref('');
const lastRequestId = ref<string | null>(null);
const states = computed<DeletionState[]>(() => {
  const member = getMemberVaultState(appState.currState) as ReturnType<typeof getMemberVaultState> & {
    deletion_states_json?: () => string;
  };
  if (!member?.deletion_states_json) return [];
  return (JSON.parse(member.deletion_states_json()) as DeletionState[]).filter(
    (state) => state.secretId === props.secret.id_str(),
  );
});
const policy = computed(() => states.value.find((state) => state.requestId === null));
const pending = computed(() => states.value.some((state) => state.clientStatus === 'Pending'));
const incoming = computed(() => states.value.find((state) => state.clientStatus === 'NeedApprove'));
watch(
  () => states.value.find((state) => state.clientStatus === 'Pending')?.requestId,
  (id) => {
    if (id) lastRequestId.value = id;
  },
  { immediate: true },
);
watch(
  () => states.value.find((state) => state.requestId === lastRequestId.value)?.clientStatus,
  (status) => {
    if (status === 'Declined') error.value = 'Deletion was declined. The Secret was kept.';
  },
);
watch(
  () => incoming.value?.requestId,
  (id) => {
    incomingOpen.value = Boolean(id);
  },
  { immediate: true },
);

const submit = async (accepted?: boolean) => {
  if (busy.value) return;
  const requestId = incoming.value?.requestId;
  if (accepted !== undefined && !requestId) return;
  busy.value = true;
  error.value = '';
  let requestStarted = false;
  try {
    if (!(await auth.authenticateWithPasskey())) throw new Error('Authentication was cancelled.');
    const manager = appState.appManager as WasmApplicationManager & {
      delete_secret?: (secret: MetaPasswordId) => Promise<void>;
      decide_delete_secret?: (requestId: string, accepted: boolean) => Promise<void>;
    };
    if (accepted === undefined) {
      if (!manager.delete_secret) throw new Error('Secret deletion is unavailable. Reload after updating.');
      requestStarted = true;
      await manager.delete_secret(props.secret);
    } else {
      if (!manager.decide_delete_secret) throw new Error('Secret deletion is unavailable. Reload after updating.');
      requestStarted = true;
      await manager.decide_delete_secret(requestId!, accepted);
    }
    confirmOpen.value = false;
    incomingOpen.value = false;
    await appState.updateState();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : 'Secret deletion failed.';
    if (requestStarted) {
      try {
        await appState.updateState();
      } catch {
        /* The store keeps cached state hidden until a successful retry. */
      }
    }
  } finally {
    busy.value = false;
  }
};
</script>

<template>
  <div class="flex flex-col items-end gap-1">
    <Button
      variant="destructive"
      size="sm"
      :disabled="busy || pending || !policy"
      :data-testid="`delete-secret-${secret.name}`"
      @click="confirmOpen = true"
    >
      {{ pending ? 'Waiting for approval…' : 'Delete' }}
    </Button>
    <Button v-if="incoming" variant="outline" size="sm" @click="incomingOpen = true">Deletion request</Button>
    <p v-if="error" role="alert" class="text-xs text-destructive">{{ error }}</p>
  </div>
  <AlertDialog
    :open="confirmOpen && !appState.isSynchronizing && !appState.syncError"
    @update:open="
      (open) => {
        if (!busy && !appState.isSynchronizing && !appState.syncError) confirmOpen = open;
      }
    "
  >
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>Delete {{ secret.name }}?</AlertDialogTitle>
        <AlertDialogDescription>
          This deletes the Secret from every Vault device, including devices when they reconnect.
          {{ policy?.requiresApproval ? 'Another device must approve the request.' : '' }}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <Button variant="outline" :disabled="busy" @click="confirmOpen = false">Cancel</Button>
        <Button variant="destructive" :disabled="busy" data-testid="confirm-delete-secret" @click="submit()">{{
          busy ? 'Submitting…' : 'Delete'
        }}</Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
  <AlertDialog
    :open="incomingOpen && !appState.isSynchronizing && !appState.syncError"
    @update:open="
      (open) => {
        if (!busy && !appState.isSynchronizing && !appState.syncError) incomingOpen = open;
      }
    "
  >
    <AlertDialogContent>
      <AlertDialogHeader>
        <AlertDialogTitle>Approve deleting {{ secret.name }}?</AlertDialogTitle>
        <AlertDialogDescription>Another Vault device requests deletion from every device.</AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <Button variant="outline" :disabled="busy" data-testid="decline-secret-deletion" @click="submit(false)"
          >Decline</Button
        >
        <Button
          variant="destructive"
          :disabled="busy"
          data-testid="approve-secret-deletion"
          :data-request-id="incoming?.requestId"
          @click="submit(true)"
          >{{ busy ? 'Submitting…' : 'Approve' }}</Button
        >
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
</template>
