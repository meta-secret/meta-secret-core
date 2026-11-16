<script setup lang="ts">
import { ref, watch } from 'vue';
import QRCodeStyling from 'qr-code-styling';
import { vaultDevices } from '@/locales/en';

const props = defineProps<{ vaultName: string }>();
const host = ref<HTMLElement | null>(null);
const failed = ref(false);

watch(
  [() => props.vaultName, host],
  ([vaultName, element], _, onCleanup) => {
    failed.value = false;
    element?.replaceChildren();
    let cancelled = false;
    onCleanup(() => {
      cancelled = true;
      element?.replaceChildren();
    });
    if (!vaultName || !element) return;

    void (async () => {
      try {
        // qr-code-styling 1.9.2's bundled Byte encoder reads only each char's
        // low byte. Supply UTF-8 bytes explicitly so Unicode names round-trip.
        const utf8Data = Array.from(new TextEncoder().encode(vaultName), (byte) => String.fromCharCode(byte)).join('');
        const code = new QRCodeStyling({
          width: 256,
          height: 256,
          type: 'svg',
          data: utf8Data,
          margin: 40,
          qrOptions: { mode: 'Byte', errorCorrectionLevel: 'M' },
          dotsOptions: { type: 'square', color: '#000000' },
          backgroundOptions: { color: '#ffffff' },
        });
        // Finish rendering before attaching; close/name changes invalidate late work.
        if (!(await code.getRawData('svg'))) throw new Error('QR unavailable');
        if (!cancelled && props.vaultName === vaultName && host.value === element) code.append(element);
      } catch {
        if (!cancelled) {
          element.replaceChildren();
          failed.value = true;
        }
      }
    })();
  },
  { flush: 'sync' },
);
</script>

<template>
  <div class="flex min-w-0 flex-col items-center gap-3">
    <div ref="host" class="vault-name-qr w-full max-w-64" role="img" :aria-label="vaultDevices.qrLabel" />
    <p v-if="failed" class="text-sm text-destructive" role="alert">{{ vaultDevices.qrError }}</p>
  </div>
</template>

<style scoped>
.vault-name-qr :deep(svg) {
  display: block;
  width: 100%;
  height: auto;
}
</style>
