<script setup lang="ts">
import CircleAction from './CircleAction.vue';
defineProps<{
  info: { label: string; title: string; detail: string; preview: boolean; removable: boolean; busy: boolean };
}>();
defineEmits<{ current: []; remove: [] }>();
</script>
<template>
  <div class="storage-info" :class="{ 'is-preview-info': info.preview }">
    <div class="storage-info-heading">
      <span id="storage-label">{{ info.label }}</span>
      <div v-if="info.preview" class="vault-storage-actions">
        <CircleAction
          kind="vault"
          label="Current"
          icon="current"
          :disabled="info.busy"
          @activate="$emit('current')"
        /><CircleAction
          v-if="info.removable"
          kind="vault"
          label="Remove"
          :icon="info.busy ? 'pending-device' : 'remove'"
          destructive
          :busy="info.busy"
          :disabled="info.busy"
          data-testid="remove-device"
          @activate="$emit('remove')"
        />
      </div>
    </div>
    <p>{{ info.title }}</p>
    <p id="storage-rule">{{ info.detail }}</p>
  </div>
</template>
