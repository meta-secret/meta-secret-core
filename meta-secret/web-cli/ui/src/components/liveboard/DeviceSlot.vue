<script setup lang="ts">
import DottedArt, { type ArtName } from './DottedArt.vue';
import type { DeviceDescriptor } from '@/features/liveboard/types';
defineProps<{
  item: {
    device?: DeviceDescriptor;
    slotId: string;
    heading: string;
    art: ArtName;
    name: string;
    state: string;
    preview?: boolean;
    removed?: boolean;
    requester?: boolean;
    responder?: boolean;
    peer?: boolean;
    disabled?: boolean;
  };
}>();
defineEmits<{ select: [targetId: string] }>();
</script>
<template>
  <button
    class="device-slot"
    :class="{
      'is-connected': item.device,
      'is-inactive': !item.device,
      'is-preview': item.preview,
      'is-removal-target': item.removed,
      'is-recovery-requester': item.requester,
      'is-response-peer': item.responder,
      'is-recovery-peer': item.peer,
    }"
    type="button"
    :data-device-id="item.device?.deviceId"
    :data-slot-id="item.slotId"
    :aria-pressed="Boolean(item.preview)"
    :disabled="item.disabled"
    @click="$emit('select', item.slotId)"
  >
    <span class="slot-heading">{{ item.heading }}</span
    ><span class="device-art"><DottedArt :name="item.art" /></span><span class="device-name">{{ item.name }}</span
    ><span class="device-state">{{ item.state }}</span>
  </button>
</template>
