<script setup lang="ts">
/** TEST ONLY: synthetic public inputs; no app routes, Core, keys, network, or persistence. */
import { computed, onBeforeUnmount, ref } from 'vue';
import { useRoute } from 'vue-router';
import LiveboardShell from '@/components/liveboard/LiveboardShell.vue';
import LiveboardHeader from '@/components/liveboard/LiveboardHeader.vue';
import LiveboardHome from '@/components/liveboard/LiveboardHome.vue';
import SecretsPage from '@/components/liveboard/SecretsPage.vue';
import ProfilePage from '@/components/liveboard/ProfilePage.vue';
import CreateSecretDialog from '@/components/liveboard/CreateSecretDialog.vue';
import InviteQrDialog from '@/components/liveboard/InviteQrDialog.vue';
import { createNotificationController } from '@/features/liveboard/notificationController';
import { createSecretController } from '@/features/liveboard/secretController';
import { createCreateController } from '@/features/liveboard/createController';
import type { IncomingRequest, RequestKind, SecretDescriptor } from '@/features/liveboard/types';
import type { VaultPresentation } from '@/features/liveboard/presentation';
const parameters = new URLSearchParams(location.search);
const route = useRoute();
const scene = computed(() => String(route.query.scene ?? 'home'));
const invite = ref(false),
  anchor = ref<HTMLElement | null>(null),
  inviteName = ref('alex@metasecret.example');
const session = { identity: 'render-session', vaultId: 'render-vault', member: true, ready: true };
const kinds: RequestKind[] = ['join', 'recover', 'delete-secret', 'remove-device'];
const requests: IncomingRequest[] = parameters.has('notice')
  ? kinds.slice(0, parameters.has('queue') ? 4 : 1).map((kind, index) => ({
      kind,
      requestId: `render-${kind}`,
      requesterId: `render-peer-${index}`,
      requesterName: 'Alex’s iPad',
      canApprove: true,
      canDecline: true,
      ...(index ? { subjectId: 'render-subject', subjectName: 'Wallet seed' } : {}),
    }))
  : [];
const notices = createNotificationController({
  authenticate: async () => false,
  decide: async () => {
    throw new Error('Render fixture must never submit a decision');
  },
});
notices.reconcile({ session, requests });
if (parameters.has('error')) notices.showError('Connection lost. Try again.');
const vault: VaultPresentation = {
  slots: [
    {
      slotId: 'phone',
      device: { deviceId: 'phone', current: true, name: 'Alex’s iPhone', canRemove: false },
      heading: 'THIS DEVICE',
      art: 'phone',
      name: 'Alex’s iPhone',
      state: 'Connected',
    },
    { slotId: 'tablet', heading: 'DEVICE 02', art: 'placeholder', name: 'Not connected', state: '' },
    { slotId: 'web', heading: 'DEVICE 03', art: 'placeholder', name: 'Not connected', state: '' },
  ],
  storage: {
    label: 'Storage model',
    title: 'This device stores each secret in full, encrypted.',
    detail: 'One device can recover the data.',
    preview: false,
    removable: false,
    busy: false,
  },
  request: null,
  canInvite: true,
};
if (parameters.has('operation')) {
  const connected = vault.slots.map((slot, index) => ({
    ...slot,
    device: {
      deviceId: slot.slotId,
      current: index === 0,
      name: ['Alex’s iPhone', 'Alex’s iPad', 'Alex’s MacBook'][index]!,
      canRemove: index !== 0,
    },
    art: (['phone', 'tablet', 'laptop'] as const)[index]!,
    requester: index === 0,
    responder: index === 1 && parameters.get('operation') !== 'waiting',
    peer: index !== 0,
    name: ['Alex’s iPhone', 'Alex’s iPad', 'Alex’s MacBook'][index]!,
    state: index ? 'Awaiting approval' : 'Recovery requested',
  }));
  vault.slots = [connected[0]!, connected[1]!, connected[2]!];
  vault.request = {
    identity: 'render-operation',
    kind: 'recover',
    phase: parameters.get('operation') as 'waiting' | 'approved' | 'declined',
    requesterId: 'phone',
    peerIds: ['tablet', 'web'],
    responderId: parameters.get('operation') === 'waiting' ? undefined : 'tablet',
    title: 'RECOVERY REQUEST',
    subject: 'Work account',
    detail: 'Waiting for Device 02 and Device 03',
  };
  if (parameters.get('kind') === 'remove-device') {
    vault.request.kind = 'remove-device';
    vault.request.title = 'REMOVE DEVICE';
    vault.request.subject = 'Alex’s MacBook';
    vault.request.peerIds = ['tablet'];
    vault.request.detail = 'Waiting for Alex’s iPad';
    vault.slots[0].state = 'Removal requested';
    vault.slots[2].state = 'Removal requested';
    vault.slots[2].removed = true;
    vault.storage = {
      label: 'Time Machine · 2 devices',
      title: 'Both devices would store full, encrypted copies.',
      detail: 'Either device could recover the data.',
      preview: true,
      removable: true,
      busy: true,
    };
  }
}
// Opt-in actual-component render scene; original cases remain unchanged.
const recoverPreviewVault = ref<VaultPresentation | null>(null);
if (parameters.has('recover-preview') && vault.request?.kind === 'recover') {
  recoverPreviewVault.value = structuredClone(vault);
}
function selectRecoverPreview(id: string) {
  const view = recoverPreviewVault.value;
  const target = view?.slots.find((slot) => slot.slotId === id)?.device;
  if (!view || !target || target.current || !target.canRemove) return;
  view.storage = { label: 'Time Machine', title: 'Both devices would store a complete encrypted copy.', detail: 'Either device could recover the data.', preview: true, removable: true, busy: false };
  view.slots.forEach((slot) => { slot.preview = slot.slotId === id; slot.removed = slot.slotId === id; });
}
function closeRecoverPreview() {
  const view = recoverPreviewVault.value;
  if (!view || view.storage.busy) return;
  view.storage = { ...vault.storage, preview: false, removable: false };
  view.slots.forEach((slot) => { slot.preview = false; slot.removed = false; });
}
const values = [
  'demo-apple-password',
  'orbit-demo-wifi-2026',
  'demo-work-password',
  'metasecret demo orbit velvet river silver meadow lantern cloud paper amber example',
  'demo-bank-password',
  'A little space for the things you want to keep.\nThis is a fictional note for the prototype.',
];
const names = ['Apple ID', 'Home Wi-Fi', 'Work account', 'Wallet seed phrase', 'Bank login', 'Private notes'];
const descriptors: SecretDescriptor[] = names.map((name, index) => ({
  passId: `render-${index}`,
  name,
  primary: { kind: index === 2 || index === 4 ? 'recover' : 'show-local' },
  canDelete: true,
}));
const secrets = createSecretController({
  showLocal: async (id) => {
    const index = Number(id.split('-').at(-1));
    return index === 3 ? { kind: 'seed', words: values[index]!.split(' ') } : { kind: 'text', text: values[index]! };
  },
  showRecovered: async () => {
    throw new Error('No Core recovery in a render fixture');
  },
  recover: async () => ({ operationId: 'render-recovery', phase: 'submitted' }),
  deleteSecret: async () => {
    throw new Error('No deletion in a render fixture');
  },
  authenticate: async () => false,
  copy: async () => {},
  reportError: notices.showError,
});
secrets.reconcile({ session, secrets: descriptors });
const presentation = (query = '') =>
  secrets.present({
    active: true,
    visibleIds: descriptors
      .filter((item) => item.name.toLowerCase().includes(query.toLowerCase()))
      .map((item) => item.passId),
    query,
    scrollTop: 0,
  });
presentation();
const create = createCreateController({
  create: async () => {
    throw new Error('No creation in a render fixture');
  },
  reconcile: async () => {
    throw new Error('No receipt in a render fixture');
  },
  readClipboard: async () => '',
  reportError: notices.showError,
});
create.reconcile(session);
if (scene.value === 'create') create.open();
// Render-only input controls. These never submit/approve a Core operation.
const renderInputs = {
  clearQrName: () => {
    inviteName.value = '';
  },
  invalidate: () => secrets.invalidate(),
  filter: () => presentation('Home Wi-Fi'),
  tombstone: () => secrets.reconcile({ session, secrets: descriptors.slice(1) }),
};
const renderWindow = window as Window & { __liveboardRenderInputs?: typeof renderInputs };
if (parameters.has('probe')) renderWindow.__liveboardRenderInputs = renderInputs;
onBeforeUnmount(() => {
  if (renderWindow.__liveboardRenderInputs === renderInputs) delete renderWindow.__liveboardRenderInputs;
  notices.dispose();
  secrets.dispose();
  create.dispose();
});
</script>
<template>
  <LiveboardShell>
    <template v-if="scene !== 'secrets' && scene !== 'profile'">
      <LiveboardHeader vault-name="alex@metasecret.example" />
      <LiveboardHome
        :vault="recoverPreviewVault ?? vault"
        :secret-count="0"
        @select-device="selectRecoverPreview"
        @current="closeRecoverPreview"
        :notifications="notices.state.value"
        :notifications-ready="session.ready"
        secrets-href="?scene=secrets"
        profile-href="?scene=profile"
        :invite-open="invite"
        @invite="
          anchor = $event;
          invite = true;
        "
        @create="create.open"
        @open-notice="notices.open"
        @decide-notice="notices.decide"
        @dismiss-error="notices.dismissError"
      />
    </template>
    <SecretsPage
      v-else-if="scene === 'secrets'"
      :view="secrets.state.value"
      back-href="?scene=home"
      @search="presentation"
      @primary="secrets.primary"
      @hide="secrets.hide"
      @copy="secrets.copy"
      @remove="secrets.remove"
    />
    <ProfilePage
      v-else
      :profile="{
        vaultName: 'alex@metasecret.example',
        deviceCount: 1,
        secretCount: 0,
        version: '3.0.0 (1)',
        alpha: false,
        resetDescription: 'Clear this device’s local data and keys.',
      }"
      back-href="?scene=home"
    />
    <template #dialogs>
      <CreateSecretDialog
        :view="create.state.value"
        :blocked="false"
        @change="create.change"
        @paste="create.paste"
        @paste-text="create.pasteText"
        @cancel="create.cancel"
      />
      <InviteQrDialog :open="invite" :anchor="anchor" :vault-name="inviteName" @close="invite = false" />
    </template>
  </LiveboardShell>
</template>
