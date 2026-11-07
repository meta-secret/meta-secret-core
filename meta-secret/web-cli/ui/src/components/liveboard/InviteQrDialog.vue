<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import QRCodeStyling from 'qr-code-styling';
import { createInviteMotion } from '@/features/liveboard/inviteMotion';
import { outsideDialog } from '@/features/liveboard/dialogMotion';
import brand from '@/design/liveboard/brand.svg';
import { liveboardTokens } from '@/design/liveboard/tokens';
import DottedLabel from './DottedLabel.vue';
import AnimatedIcon from './AnimatedIcon.vue';
const props = defineProps<{ open: boolean; anchor: HTMLElement | null; vaultName: string }>();
const emit = defineEmits<{ close: []; error: [message: string] }>();
const dialog = ref<HTMLDialogElement | null>(null),
  body = ref<HTMLElement | null>(null),
  code = ref<HTMLElement | null>(null);
const loading = ref(false);
let motion: ReturnType<typeof createInviteMotion> | null = null;
let overflow: string | null = null,
  revision = 0;
watch(
  [() => props.open, () => props.anchor],
  async ([open, anchor]) => {
    const token = ++revision;
    await nextTick();
    if (token !== revision || !dialog.value || !body.value) return;
    if (open && anchor) {
      motion?.cancel();
      motion = createInviteMotion({ dialog: dialog.value, body: body.value, anchor });
      if (overflow === null) {
        overflow = document.body.style.overflow;
        document.body.style.overflow = 'hidden';
      }
      await motion.open();
    } else {
      await motion?.close();
      code.value?.replaceChildren();
      if (token === revision && overflow !== null) {
        document.body.style.overflow = overflow;
        overflow = null;
      }
    }
  },
  { immediate: true },
);
watch(
  [() => props.open, () => props.vaultName, code],
  ([open, vaultName, host], _, cleanup) => {
    let cancelled = false;
    cleanup(() => {
      cancelled = true;
    });
    loading.value = false;
    if (!vaultName) host?.replaceChildren();
    if (!open || !vaultName || !host) return;
    host.replaceChildren();
    loading.value = true;
    void (async () => {
      try {
        const data = Array.from(new TextEncoder().encode(vaultName), (byte) => String.fromCharCode(byte)).join('');
        const qr = new QRCodeStyling({
          width: 208,
          height: 208,
          type: 'svg',
          data,
          margin: 12,
          qrOptions: { mode: 'Byte', errorCorrectionLevel: 'H' },
          image: brand,
          imageOptions: { crossOrigin: 'anonymous', imageSize: 0.18, margin: 4, hideBackgroundDots: true },
          dotsOptions: { type: 'square', color: '#000000' },
          backgroundOptions: { color: liveboardTokens.liveboardColorForeground },
        });
        await qr.getRawData('svg');
        if (!cancelled && props.vaultName === vaultName) qr.append(host);
      } catch {
        if (!cancelled) {
          emit('error', 'Could not generate connection QR.');
          emit('close');
        }
      } finally {
        if (!cancelled) loading.value = false;
      }
    })();
  },
  { flush: 'post' },
);
function resized() {
  if (props.open) {
    motion?.cancel();
    motion?.fit();
    if (dialog.value) dialog.value.dataset.phase = 'open';
  }
}
onMounted(() => window.addEventListener('resize', resized));
onBeforeUnmount(() => {
  revision++;
  motion?.cancel();
  dialog.value?.close();
  code.value?.replaceChildren();
  window.removeEventListener('resize', resized);
  if (overflow !== null) document.body.style.overflow = overflow;
});
</script>
<template>
  <dialog
    ref="dialog"
    class="invite-popover"
    aria-labelledby="invite-title"
    aria-describedby="invite-instruction"
    @cancel.prevent="emit('close')"
    @click="dialog && outsideDialog($event, dialog) && emit('close')"
  >
    <button class="invite-close" type="button" aria-label="Close connection QR" autofocus @click="emit('close')">
      <AnimatedIcon name="close" />
    </button>
    <div ref="body" class="invite-body">
      <h2 id="invite-title"><DottedLabel text="ADD DEVICE" /></h2>
      <div ref="code" class="invite-code" role="img" :aria-label="`Connection QR for ${vaultName}`" />
      <div v-if="loading" class="invite-code invite-status" role="status" aria-label="Generating QR">
        <span class="notice-loader" aria-hidden="true"><i v-for="index in 5" :key="index" /></span>
      </div>
      <div class="invite-caption">
        <p id="invite-instruction">Scan on your new device</p>
        <span>{{ vaultName }}</span>
      </div>
    </div>
  </dialog>
</template>
<style scoped>
.invite-code :deep(svg) {
  display: block;
  width: 100%;
  height: 100%;
  border-radius: inherit;
}
.invite-status {
  display: grid;
  place-items: center;
  background: transparent;
  color: var(--liveboard-color-foreground);
  font-size: var(--liveboard-font-size-foot);
}
</style>
