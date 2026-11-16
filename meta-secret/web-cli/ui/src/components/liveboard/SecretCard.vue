<script setup lang="ts">
import { computed, ref, type DeepReadonly } from 'vue';
import type { SecretCardView } from '@/features/liveboard/types';
import { useMeasuredHeight } from '@/features/liveboard/motion';
import DottedLabel from './DottedLabel.vue';
import AnimatedIcon from './AnimatedIcon.vue';
import SecretValue from './SecretValue.vue';
import CircleAction from './CircleAction.vue';
const props = defineProps<{ card: DeepReadonly<SecretCardView>; index: number; blocked: boolean }>();
defineEmits<{ primary: [id: string]; hide: [id: string]; copy: [id: string]; remove: [id: string] }>();
const root = ref<HTMLElement | null>(null);
useMeasuredHeight(root, 'secretHeight');
const waiting = computed(() => ['waiting', 'revealing', 'deleting'].includes(props.card.phase));
const label = computed(() =>
  waiting.value
    ? 'Waiting'
    : props.card.phase === 'revealed'
      ? 'Hide'
      : props.card.secret.primary.kind === 'recover'
        ? 'Recover'
        : 'Show',
);
const icon = computed(() => (waiting.value ? 'pending' : props.card.phase === 'revealed' ? 'hide' : 'show'));
</script>
<template>
  <article
    ref="root"
    class="tile secret-card"
    :class="{
      'is-revealed': card.value,
      'is-waiting': card.phase === 'waiting',
      'is-deleting': card.phase === 'deleting',
    }"
    role="listitem"
    :data-secret-id="card.secret.passId"
    :data-phase="card.phase"
    :style="{ '--secret-index': index }"
  >
    <div class="secret-card-heading">
      <DottedLabel class="secret-number" :text="String(index + 1).padStart(2, '0')" /><AnimatedIcon
        class="secret-lock"
        name="lock"
      />
    </div>
    <h2 class="secret-name">{{ card.secret.name }}</h2>
    <SecretValue :card="card" :index="index" />
    <div class="secret-card-actions">
      <CircleAction
        :label="label"
        :icon="icon"
        :action="card.phase === 'revealed' ? 'hide' : label.toLowerCase()"
        :disabled="
          blocked || waiting || card.secret.primary.kind === 'blocked' || card.secret.primary.kind === 'waiting'
        "
        :busy="waiting"
        :data-testid="`secret-primary-action-${card.secret.name}`"
        @activate="card.phase === 'revealed' ? $emit('hide', card.secret.passId) : $emit('primary', card.secret.passId)"
      />
      <CircleAction
        :label="card.phase === 'deleting' ? 'Waiting' : 'Delete'"
        :icon="card.phase === 'deleting' ? 'pending' : 'delete'"
        :action="card.phase === 'deleting' ? 'delete-waiting' : 'delete'"
        :disabled="blocked || waiting || !card.secret.canDelete"
        :busy="card.phase === 'deleting'"
        :data-testid="`delete-secret-${card.secret.name}`"
        @activate="$emit('remove', card.secret.passId)"
      />
      <CircleAction
        v-if="card.value"
        :label="card.copied ? 'Copied' : 'Copy'"
        icon="copy"
        action="copy"
        :disabled="blocked"
        @activate="$emit('copy', card.secret.passId)"
      />
    </div>
  </article>
</template>
