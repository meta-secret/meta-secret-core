<script setup lang="ts">
import { RouterLink } from 'vue-router';
import type { DeepReadonly } from 'vue';
import type { VaultPresentation, NotificationReceiptPresentation } from '@/features/liveboard/presentation';
import type { NotificationView } from '@/features/liveboard/types';
import VaultTile from './VaultTile.vue';
import NotificationTile from './NotificationTile.vue';
import DottedLabel from './DottedLabel.vue';
import DottedArt from './DottedArt.vue';
withDefaults(
  defineProps<{
    vault: VaultPresentation;
    secretCount: number;
    notifications: DeepReadonly<NotificationView>;
    notificationsReady: boolean;
    notificationReceipt?: NotificationReceiptPresentation | null;
    inviteOpen?: boolean;
    canCreate?: boolean;
    secretsHref: string;
    profileHref: string;
  }>(),
  { canCreate: true, notificationReceipt: undefined },
);
defineEmits<{
  selectDevice: [id: string];
  current: [];
  removeDevice: [];
  invite: [anchor: HTMLElement];
  create: [];
  openNotice: [];
  decideNotice: [accepted: boolean];
  dismissError: [];
}>();
</script>
<template>
  <div class="board">
    <VaultTile
      :vault="vault"
      :invite-open="inviteOpen"
      @select="$emit('selectDevice', $event)"
      @current="$emit('current')"
      @remove="$emit('removeDevice')"
      @invite="$emit('invite', $event)"
    />
    <RouterLink
      class="tile tile-count tile-secrets"
      :to="secretsHref"
      aria-label="Open Secrets"
      data-testid="liveboard-secrets-open"
      ><span class="tile-heading"><span>02</span> SECRETS</span
      ><DottedLabel class="count" :text="String(secretCount).padStart(2, '0')" /><span class="tile-foot"
        >Secrets</span
      ></RouterLink
    >
    <button
      class="tile tile-action tile-create"
      type="button"
      aria-label="Create a secret"
      aria-haspopup="dialog"
      data-testid="create-secret-open"
      :disabled="!canCreate"
      @click="$emit('create')"
    >
      <span class="tile-heading"><span>03</span> CREATE SECRET</span
      ><span class="action-art"><DottedArt name="secret" /></span
      ><DottedLabel class="action-name" text="CREATE SECRET" />
    </button>
    <NotificationTile
      :view="notifications"
      :ready="notificationsReady"
      :receipt="notificationReceipt"
      @open="$emit('openNotice')"
      @decide="$emit('decideNotice', $event)"
      @dismiss-error="$emit('dismissError')"
    />
    <RouterLink class="tile tile-profile" :to="profileHref" aria-label="Open Profile"
      ><span class="tile-heading"><span>05</span> PROFILE</span
      ><span class="utility-content"
        ><span class="utility-art"><DottedArt name="settings" /></span
        ><span class="utility-copy"
          ><DottedLabel text="PROFILE" /><span class="tile-foot">Vault &amp; app details</span></span
        ></span
      ></RouterLink
    >
  </div>
</template>
<style scoped>
a.tile {
  text-decoration: none;
}
</style>
