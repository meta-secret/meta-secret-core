<script setup lang="ts">
import { computed, ref, type DeepReadonly } from 'vue';
import type { NotificationView, RequestKind } from '@/features/liveboard/types';
import type { NotificationReceiptPresentation } from '@/features/liveboard/presentation';
import { useMeasuredHeight } from '@/features/liveboard/motion';
import AnimatedIcon from './AnimatedIcon.vue';
import DottedLabel from './DottedLabel.vue';
import CircleAction from './CircleAction.vue';
const props = defineProps<{
  view: DeepReadonly<NotificationView>;
  ready: boolean;
  receipt?: NotificationReceiptPresentation | null;
}>();
defineEmits<{ open: []; decide: [accepted: boolean]; dismissError: [] }>();
const root = ref<HTMLElement | null>(null);
useMeasuredHeight(root, 'notificationHeight');
const item = computed(() => props.view.requests[0]);
const state = computed(() =>
  props.view.error
    ? 'error'
    : props.view.busy
      ? 'sending'
      : item.value
        ? props.view.expanded
          ? 'actions'
          : 'request'
        : 'empty',
);
const summaries: Record<RequestKind, string> = {
  join: 'Wants to join your Vault',
  recover: 'Requests recovery',
  'delete-secret': 'Requests deletion',
  'remove-device': 'Requests device removal',
};
const approving: Record<RequestKind, string> = {
  join: 'Approving device…',
  recover: 'Approving recovery…',
  'delete-secret': 'Approving deletion…',
  'remove-device': 'Approving removal…',
};
const titles: Record<RequestKind, string> = {
  join: 'JOIN REQUEST',
  recover: 'RECOVER SECRET',
  'delete-secret': 'DELETE SECRET',
  'remove-device': 'REMOVE DEVICE',
};
</script>
<template>
  <section
    ref="root"
    class="tile tile-notifications"
    :data-state="state"
    aria-labelledby="notifications-title"
    data-testid="liveboard-notifications"
  >
    <span id="notifications-title" class="tile-heading"><span>04</span> NOTIFICATIONS</span>
    <div class="notification-view" :aria-live="view.error ? 'assertive' : 'polite'" aria-atomic="true">
      <span class="notification-icon" :class="{ 'is-error': view.error }"
        ><AnimatedIcon :name="view.error ? 'error' : 'bell'" /><span
          v-if="item && !view.error"
          class="notification-count"
          >{{ view.requests.length }}</span
        ></span
      >
      <div class="notification-copy" :class="{ 'is-error': view.error }">
        <template v-if="view.error"
          ><DottedLabel class="notification-title" text="ERROR" /><span class="notification-detail">{{
            view.error
          }}</span></template
        >
        <template v-else-if="receipt"
          ><DottedLabel class="notification-title" :text="receipt.outcome.toUpperCase()" /><span
            class="notification-detail"
            >{{ receipt.requesterName }}<template v-if="receipt.subjectName"><br />{{ receipt.subjectName }}</template
            ><template v-if="receipt.outcome === 'handled'"><br />Handled on another device</template></span
          ></template
        >
        <template v-else-if="item">
          <DottedLabel v-if="!view.expanded" class="notification-title" :text="titles[item.kind]" />
          <span v-if="!view.expanded" class="notification-detail"
            >{{ item.requesterName }}<br />{{ summaries[item.kind]
            }}{{ item.subjectName ? ` · ${item.subjectName}` : '' }}</span
          >
          <div v-else class="request-identity">
            <span class="request-device">{{ item.requesterName }}</span
            ><span v-if="item.subjectName" class="notification-detail request-subject">{{ item.subjectName }}</span>
          </div>
          <template v-if="view.busy"
            ><span class="notice-loader" aria-hidden="true"><i v-for="index in 5" :key="index" /></span
            ><span class="notification-detail">{{
              view.pendingDecision?.accepted ? approving[item.kind] : 'Declining request…'
            }}</span></template
          >
          <div
            v-if="view.expanded && !view.busy"
            class="notification-actions"
            :data-request-id="item.requestId"
            :data-request-kind="item.kind"
          >
            <CircleAction
              kind="notice"
              icon="approve"
              label="Approve"
              :disabled="!ready || view.busy || !item.canApprove"
              :data-testid="`approve-${item.kind}-request`"
              @activate="$emit('decide', true)"
            />
            <CircleAction
              kind="notice"
              icon="decline"
              label="Decline"
              :disabled="!ready || view.busy || !item.canDecline"
              :data-testid="`decline-${item.kind}-request`"
              @activate="$emit('decide', false)"
            />
          </div>
        </template>
        <template v-else
          ><DottedLabel class="notification-title" text="NO ACTIONS" /><span class="notification-detail"
            >No actions waiting</span
          ></template
        >
      </div>
    </div>
    <button
      v-if="view.error || (!view.expanded && !receipt)"
      class="notification-hit"
      type="button"
      :aria-label="
        view.error
          ? 'Dismiss error'
          : item
            ? `Open ${titles[item.kind]} from ${item.requesterName}`
            : 'Notifications: no actions waiting'
      "
      :disabled="!view.error && (!ready || !item || view.busy)"
      @click="view.error ? $emit('dismissError') : $emit('open')"
    />
  </section>
</template>
