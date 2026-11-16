<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { VaultPresentation } from '@/features/liveboard/presentation';
import { useReducedMotion } from '@/features/liveboard/motion';
import DottedLabel from './DottedLabel.vue';
import AnimatedIcon from './AnimatedIcon.vue';
import DeviceSlot from './DeviceSlot.vue';
import StorageInfo from './StorageInfo.vue';
import RequestLinks from './RequestLinks.vue';
const props = defineProps<{ vault: VaultPresentation; inviteOpen?: boolean }>();
const emit = defineEmits<{ select: [id: string]; current: []; remove: []; invite: [anchor: HTMLElement] }>();
const host = ref<HTMLElement | null>(null),
  plus = ref<HTMLElement | null>(null);
const anchors = ref<Record<string, { x: number; y: number }>>({});
const bounds = ref({ width: 1, height: 1 });
const reduced = useReducedMotion();
let observer: ResizeObserver | undefined;
function measure() {
  const element = host.value;
  if (!element) return;
  const area = element.getBoundingClientRect();
  bounds.value = { width: area.width, height: area.height };
  const points: Record<string, { x: number; y: number }> = Object.create(null);
  for (const slot of element.querySelectorAll<HTMLElement>('[data-device-id]')) {
    const art = slot.querySelector('.device-art')?.getBoundingClientRect();
    if (art && slot.dataset.deviceId)
      points[slot.dataset.deviceId] = {
        x: art.left + art.width / 2 - area.left,
        y: art.top + art.height * 0.2 - area.top,
      };
  }
  anchors.value = points;
}
onMounted(() => {
  measure();
  if (typeof ResizeObserver !== 'undefined' && host.value) {
    observer = new ResizeObserver(measure);
    observer.observe(host.value);
  }
});
watch(
  () => props.vault,
  () => {
    void nextTick(measure);
  },
  { deep: true },
);
onBeforeUnmount(() => observer?.disconnect());
</script>
<template>
  <section
    class="tile tile-vault"
    :class="{ 'is-recovery-preview': vault.request }"
    :data-recovery-phase="vault.request?.phase"
    :data-recovery-outcome="
      vault.request?.phase === 'approved' ? 'approve' : vault.request?.phase === 'declined' ? 'decline' : undefined
    "
    aria-label="Vault devices"
  >
    <div class="vault-heading">
      <DottedLabel text="VAULT" /><button
        ref="plus"
        class="add-device-trigger"
        type="button"
        aria-label="Add a device to Vault"
        aria-haspopup="dialog"
        :aria-expanded="Boolean(inviteOpen)"
        :disabled="!vault.canInvite"
        data-testid="add-device"
        @click="plus && emit('invite', plus)"
      >
        <AnimatedIcon name="plus" />
      </button>
    </div>
    <div class="vault-body">
      <div ref="host" class="device-slots" aria-label="Vault devices and Time Machine previews">
        <RequestLinks
          v-if="vault.request"
          :request="vault.request"
          :anchors="anchors"
          :bounds="bounds"
          :reduced="reduced"
        /><DeviceSlot v-for="slot in vault.slots" :key="slot.slotId" :item="slot" @select="emit('select', $event)" />
      </div>
      <div v-if="vault.request" class="vault-recovery" role="status">
        <span class="vault-recovery-label"><i aria-hidden="true" />{{ vault.request.title }}</span>
        <p>{{ vault.request.subject }}</p>
        <span>{{ vault.request.detail }}</span>
      </div>
      <StorageInfo
        v-if="!vault.request || vault.request.kind === 'remove-device' || (vault.request.kind === 'recover' && vault.storage.preview)"
        :info="vault.storage"
        @current="emit('current')"
        @remove="emit('remove')"
      />
    </div>
  </section>
</template>
