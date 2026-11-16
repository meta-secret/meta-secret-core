<script setup lang="ts">
import OrbitIcon from './OrbitIcon.vue';
withDefaults(
  defineProps<{
    icon: 'apple' | 'google' | 'create' | 'join' | 'enter' | 'qr' | 'back' | 'request' | 'ok' | 'approved';
    label: string;
    disabled?: boolean;
    provider?: boolean;
    motion?: boolean;
    hideLabel?: boolean;
  }>(),
  { motion: false },
);
defineEmits<{ activate: [] }>();
</script>
<template>
  <button
    type="button"
    class="orbit-control"
    :class="[{ 'is-provider': provider, 'has-motion': motion }, `icon-${icon}`]"
    :disabled="disabled"
    :aria-label="label"
    @click="$emit('activate')"
  >
    <span class="orbit-circle"><OrbitIcon :name="icon" /><slot /></span>
    <span v-if="!hideLabel" class="orbit-control-label">{{ label }}</span>
  </button>
</template>
