<script setup lang="ts">
/** Isolated design fixture. Never imported by an application route or production adapter. */
import { computed, ref } from 'vue';
import { useRoute } from 'vue-router';
import LiveboardShell from '@/components/liveboard/LiveboardShell.vue';
import LiveboardHeader from '@/components/liveboard/LiveboardHeader.vue';
import LiveboardHome from '@/components/liveboard/LiveboardHome.vue';
import SecretsPage from '@/components/liveboard/SecretsPage.vue';
import ProfilePage from '@/components/liveboard/ProfilePage.vue';
import CreateSecretDialog from '@/components/liveboard/CreateSecretDialog.vue';
import InviteQrDialog from '@/components/liveboard/InviteQrDialog.vue';
import ResetLocalDialog from '@/components/liveboard/ResetLocalDialog.vue';
import type { NotificationView, SecretView, CreateView, CreateDraft } from '@/features/liveboard/types';
import type { VaultPresentation } from '@/features/liveboard/presentation';
const parameters = new URLSearchParams(location.search);
const route = useRoute();
const scene = computed(() => String(route.query.scene ?? 'home'));
const invite = ref(false),
  anchor = ref<HTMLElement | null>(null),
  reset = ref(false);
const notices = ref<NotificationView>({
  requests: parameters.get('notice')
    ? [
        {
          kind: 'join',
          requestId: 'fixture-join',
          requesterId: 'tablet',
          requesterName: 'Alex’s iPad',
          canApprove: true,
          canDecline: true,
        },
      ]
    : [],
  selected: null,
  expanded: false,
  busy: false,
  error: parameters.get('error') ? 'Could not sync Vault.' : null,
});
const vault = computed<VaultPresentation>(() => ({
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
}));
const secrets = ref<SecretView>({
  cards: Object.fromEntries(
    ['Apple ID', 'Home Wi-Fi', 'Work account', 'Wallet seed phrase', 'Bank login', 'Private notes'].map(
      (name, index) => [
        `fixture-pass-${index}`,
        {
          secret: { passId: `fixture-pass-${index}`, name, primary: { kind: 'show-local' }, canDelete: true },
          phase: 'masked',
          value: null,
          copied: false,
        },
      ],
    ),
  ),
  presentation: { active: true, visibleIds: [], query: '', scrollTop: 0 },
  blocked: false,
});
const create = ref<CreateView>({
  open: scene.value === 'create',
  draft: { mode: 'secret', name: '', value: '', wordCount: 12, words: Array<string>(24).fill('') },
  phase: 'idle',
  receipt: null,
  pasteHint: '',
});
function change(patch: Partial<CreateDraft>) {
  create.value.draft = { ...create.value.draft, ...patch };
}
function reveal(id: string) {
  const card = secrets.value.cards[id];
  if (card) {
    card.phase = 'revealed';
    card.value = { kind: 'text', text: 'Fictional render value' };
  }
}
function hide(id: string) {
  const card = secrets.value.cards[id];
  if (card) {
    card.conceal = true;
    card.value = null;
    card.phase = 'masked';
  }
}
function openNotice() {
  const item = notices.value.requests[0];
  if (item) {
    notices.value.selected = { kind: item.kind, requestId: item.requestId };
    notices.value.expanded = true;
  }
}
</script>
<template>
  <LiveboardShell
    ><template v-if="scene !== 'secrets' && scene !== 'profile'"
      ><LiveboardHeader vault-name="alex@metasecret.example" /><LiveboardHome
        :vault="vault"
        :secret-count="0"
        :notifications="notices"
        :notifications-ready="true"
        secrets-href="?scene=secrets"
        profile-href="?scene=profile"
        :invite-open="invite"
        @invite="
          anchor = $event;
          invite = true;
        "
        @create="create.open = true"
        @open-notice="openNotice"
        @dismiss-error="notices.error = null" /></template
    ><SecretsPage
      v-else-if="scene === 'secrets'"
      :view="secrets"
      back-href="?scene=home"
      @search="secrets.presentation.query = $event"
      @primary="reveal"
      @hide="hide"
    /><ProfilePage
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
      @reset="reset = true"
    />
    <template #dialogs
      ><CreateSecretDialog
        :view="create"
        :blocked="false"
        @change="change"
        @cancel="create.open = false" /><InviteQrDialog
        :open="invite"
        :anchor="anchor"
        vault-name="alex@metasecret.example"
        @close="invite = false" /><ResetLocalDialog :open="reset" :busy="false" @cancel="reset = false"
    /></template>
  </LiveboardShell>
</template>
