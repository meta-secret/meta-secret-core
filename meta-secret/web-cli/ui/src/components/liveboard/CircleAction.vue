<script setup lang="ts">
import AnimatedIcon, { type IconName } from './AnimatedIcon.vue';
withDefaults(
  defineProps<{
    label: string;
    icon: IconName;
    kind?: 'secret' | 'vault' | 'notice' | 'create';
    action?: string;
    disabled?: boolean;
    busy?: boolean;
    destructive?: boolean;
    type?: 'button' | 'submit';
  }>(),
  { kind: 'secret', type: 'button', action: undefined },
);
defineEmits<{ activate: [] }>();
</script>
<template>
  <button
    :type="type"
    :class="[`${kind}-control`, { 'is-destructive': destructive }]"
    :data-secret-action="action"
    :data-waiting="busy || undefined"
    :disabled="disabled"
    :aria-busy="busy || undefined"
    @click="$emit('activate')"
  >
    <span :class="kind === 'secret' ? 'secret-control-circle' : kind === 'notice' ? 'notice-circle' : undefined"
      ><AnimatedIcon :name="icon"
    /></span>
    <small v-if="kind === 'vault' || kind === 'create'">{{ label }}</small
    ><span v-else>{{ label }}</span>
  </button>
</template>
