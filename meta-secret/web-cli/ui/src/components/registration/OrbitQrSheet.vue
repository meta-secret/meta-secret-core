<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { orbitTokens } from '@/design/orbit/tokens';
import { createCameraScanner, type CameraScannerFactory } from '@/features/registration/cameraScanner';
import OrbitControl from './OrbitControl.vue';
import OrbitIcon from './OrbitIcon.vue';
const props = defineProps<{ origin: { x: number; y: number; size: number }; cameraFactory?: CameraScannerFactory }>();
const emit = defineEmits<{ close: []; scan: [name: string] }>();
const sheet = ref<HTMLElement>();
const video = ref<HTMLVideoElement>();
const error = ref('');
const revealed = ref(false);
let scanner: ReturnType<typeof createCameraScanner> | undefined;
let closing = false;
let disposed = false;
let animation: Animation | undefined;
let priorFocus: HTMLElement | null = null;
const stop = () => {
  scanner?.stop();
  scanner = undefined;
};
const reduced = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;
async function morph(opening: boolean) {
  if (!sheet.value || !sheet.value.animate || reduced()) return;
  const bounds = sheet.value.getBoundingClientRect();
  const parent = sheet.value.offsetParent?.getBoundingClientRect();
  const expanded = {
    left: `${bounds.left - (parent?.left ?? 0)}px`,
    top: `${bounds.top - (parent?.top ?? 0)}px`,
    width: `${bounds.width}px`,
    height: `${bounds.height}px`,
    borderRadius: `${orbitTokens.orbitDimensionQrRadius}px`,
  };
  const circle = {
    left: `${props.origin.x}px`,
    top: `${props.origin.y}px`,
    width: `${props.origin.size}px`,
    height: `${props.origin.size}px`,
    borderRadius: '50%',
  };
  animation = sheet.value.animate(opening ? [circle, expanded] : [expanded, circle], {
    duration: orbitTokens.orbitDurationQr,
    easing: 'cubic-bezier(.24,.75,.23,1)',
  });
  await animation.finished.catch(() => undefined);
}
async function close(name?: string) {
  if (closing) return;
  closing = true;
  stop();
  revealed.value = false;
  await morph(false);
  if (disposed) return;
  if (name) emit('scan', name);
  else {
    emit('close');
    await nextTick();
    priorFocus?.focus({ preventScroll: true });
  }
}
function onVisibility() {
  if (document.visibilityState !== 'visible') void close();
}
function focusBack() {
  sheet.value?.querySelector<HTMLButtonElement>('button')?.focus({ preventScroll: true });
}
onMounted(async () => {
  priorFocus = document.activeElement as HTMLElement | null;
  document.addEventListener('visibilitychange', onVisibility);
  await nextTick();
  await morph(true);
  if (closing || disposed) return;
  revealed.value = true;
  focusBack();
  if (video.value) {
    scanner = (props.cameraFactory ?? createCameraScanner)(
      video.value,
      (name) => {
        void close(name);
      },
      (message) => {
        error.value = message;
      },
    );
    await scanner.start();
  }
});
onBeforeUnmount(() => {
  disposed = true;
  stop();
  animation?.cancel();
  document.removeEventListener('visibilitychange', onVisibility);
});
</script>
<template>
  <div class="orbit-qr-layer" @keydown.esc.prevent="close()" @keydown.tab.prevent="focusBack">
    <section ref="sheet" class="orbit-qr-sheet" role="dialog" aria-modal="true" aria-label="Scan a Vault QR code">
      <span v-if="!revealed" class="orbit-qr-morph-icon"><OrbitIcon name="qr" /></span>
      <div class="orbit-qr-body" :class="{ revealed }">
        <div class="orbit-camera">
          <video ref="video" muted playsinline aria-label="Camera preview" />
          <div class="orbit-camera-corners" aria-hidden="true"><i /><i /><i /><i /></div>
          <p v-if="error" class="orbit-camera-error" role="alert">{{ error }}</p>
        </div>
        <OrbitControl icon="back" label="Back" @activate="close()" />
      </div>
    </section>
  </div>
</template>
