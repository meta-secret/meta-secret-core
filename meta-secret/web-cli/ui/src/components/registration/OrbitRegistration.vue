<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { createRegistrationController, type RegistrationPort } from '@/features/registration/registrationController';
import { createWasmRegistrationAdapter } from '@/features/registration/wasmRegistrationAdapter';
import {
  emailProviderCapabilities,
  selectAppleEmail,
  selectGoogleEmail,
  warmEmailProviders,
} from '@/features/registration/emailProvider';
import type { CameraScannerFactory } from '@/features/registration/cameraScanner';
import { readWebQrAvailability } from '@/features/registration/qrAvailability';
import { orbitTokens } from '@/design/orbit/tokens';
import OrbitScale from './OrbitScale.vue';
import OrbitScene from './OrbitScene.vue';
import OrbitControl from './OrbitControl.vue';
import OrbitLoader from './OrbitLoader.vue';
import OrbitQrSheet from './OrbitQrSheet.vue';
import OrbitIcon from './OrbitIcon.vue';
import '@/design/orbit/orbit.css';
import './orbit.css';
const props = defineProps<{ port?: RegistrationPort; cameraFactory?: CameraScannerFactory }>();
const emit = defineEmits<{ ready: [] }>();
const controller = createRegistrationController(props.port ?? createWasmRegistrationAdapter());
const { state } = controller;
const surface = ref<HTMLElement>();
const input = ref<HTMLInputElement>();
const draft = ref('');
const providerBusy = ref(false);
const merging = ref(false);
const qrAvailable = ref(false);
const qrOrigin = ref<{ x: number; y: number; size: number }>();
const expanded = ref(false);
let layoutFrame: number | undefined;
let layoutQueued = false;
let statusGap = 83.25;
let mergeTimer: ReturnType<typeof setTimeout> | undefined;
let qrProbeGeneration = 0;
let disposed = false;
let appleSelection: AbortController | undefined;
async function refreshQrAvailability() {
  const generation = ++qrProbeGeneration;
  const available = await readWebQrAvailability();
  if (!disposed && generation === qrProbeGeneration) qrAvailable.value = available;
}
function onVisibilityChange() {
  if (document.visibilityState === 'visible') void refreshQrAvailability();
}
const waiting = computed(() => ['pending', 'requesting'].includes(state.value.phase));
const loading = computed(() =>
  ['checking', 'creating', 'requesting', 'pending', 'syncing'].includes(state.value.phase),
);
const subtitle = computed(() => {
  if (state.value.message) return state.value.message;
  return {
    choice: 'Your network of trust',
    checking: 'Checking availability…',
    available: 'This name is available',
    exists: 'This name is taken',
    editing: '',
    join: 'This Vault exists. Join to continue.',
    creating: 'Creating your Vault…',
    requesting: 'Awaiting approval…',
    pending: 'Awaiting approval…',
    syncing: 'Syncing your Vault…',
    error: '',
  }[state.value.phase];
});
// Presentation measurements stay in the bounded Web frame; no hidden status or Core state is introduced.
async function measureLayout() {
  expanded.value = window.innerWidth >= 900 && window.innerHeight >= 600;
  await nextTick();
  if (disposed || !surface.value) return;
  const layout = surface.value.querySelector<HTMLElement>('.orbit-layout');
  if (!layout || !expanded.value) return;
  const scale = 0.9 * Math.max(0.4, Math.min(1, (window.innerWidth - 110) / 900, (window.innerHeight - 190) / 500));
  layout.style.setProperty('--orbit-balanced-scale', String(scale));
  const bounds = layout.getBoundingClientRect();
  if (!bounds.width) return; // Non-rendering component tests do not supply font geometry.
  const context = document.createElement('canvas').getContext('2d');
  if (!context) return;
  const family = getComputedStyle(surface.value).fontFamily;
  const name = state.value.step === 3 ? draft.value || state.value.name : state.value.name;
  const text = name || 'Enter your email';
  const widthAt = (size: number) => {
    context.font = `400 ${size}px ${family}`;
    return context.measureText(text).width - 0.04 * size * Array.from(text).length;
  };
  let size = 42.3;
  while (size > 16 && Math.ceil(widthAt(size) / 700) * size * 1.2 > 96.65) size = Math.max(16, size - 1);
  layout.style.setProperty('--orbit-identity-font', `${size}px`);
  const live = layout.querySelector<HTMLElement>('.orbit-turn:not([aria-hidden="true"]):not([inert])');
  const heading = live?.querySelector<HTMLElement>('.orbit-heading.is-identity h1');
  // Actual wrapping may differ from canvas line estimates; this loop has at most 27 bounded steps.
  while (heading && size > 16 && heading.getBoundingClientRect().height / scale > 96.65) {
    size = Math.max(16, size - 1);
    layout.style.setProperty('--orbit-identity-font', `${size}px`);
  }
  let width = widthAt(size);
  if (heading && heading.getBoundingClientRect().height / scale <= size * 1.2 + 1) {
    const range = document.createRange();
    range.selectNodeContents(heading);
    width = range.getBoundingClientRect().width / scale;
  }
  layout.style.setProperty('--orbit-field-width', `${Math.max(372.24, Math.min(630, width))}px`);
  layout.style.setProperty('--orbit-field-top', `${142 + size * 0.6 - 42.3}px`);
  const status = live?.querySelector<HTMLElement>('.orbit-heading p');
  if (status) {
    const range = document.createRange();
    range.selectNodeContents(status);
    const lineBottom = (range.getBoundingClientRect().bottom - status.getBoundingClientRect().top) / scale;
    const titleHeight = heading ? heading.getBoundingClientRect().height / scale : size * 1.2;
    statusGap = Math.max(24, 320 - (142 + titleHeight + 18 + lineBottom));
  }
  layout.style.setProperty('--orbit-status-action-gap', `${statusGap}px`);
  updateQrOrigin();
}
function scheduleLayout() {
  if (disposed || layoutQueued) return;
  layoutQueued = true;
  const run = () => {
    layoutQueued = false;
    layoutFrame = undefined;
    if (!disposed) void measureLayout();
  };
  if (typeof requestAnimationFrame === 'function') layoutFrame = requestAnimationFrame(run);
  else queueMicrotask(run);
}
watch(() => [state.value.step, state.value.phase, state.value.name, draft.value], scheduleLayout, { flush: 'post' });
watch(
  () => state.value.ready,
  (ready) => {
    if (ready) emit('ready');
  },
);
watch(
  () => state.value.step,
  (step) => {
    if (step === 3) draft.value = state.value.name;
  },
);
async function provider(name: 'apple' | 'google') {
  if (providerBusy.value) return;
  providerBusy.value = true;
  try {
    if (name === 'apple') appleSelection = new AbortController();
    const email = await (name === 'apple' ? selectAppleEmail(appleSelection!.signal) : selectGoogleEmail());
    if (!disposed) await controller.chooseName(email);
  } catch (error) {
    if (!disposed) controller.providerError(error instanceof Error ? error.message : 'Sign-in was not completed.');
  } finally {
    appleSelection = undefined;
    providerBusy.value = false;
  }
}
async function request() {
  if (merging.value) return;
  merging.value = true;
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (!reduced)
    mergeTimer = setTimeout(() => {
      merging.value = false;
    }, orbitTokens.orbitDurationJoinFusion);
  else merging.value = false;
  await controller.request();
}
function updateQrOrigin() {
  if (!qrOrigin.value || !surface.value) return;
  const rect = surface.value.querySelector('.icon-qr .orbit-circle')?.getBoundingClientRect();
  const bounds = surface.value.getBoundingClientRect();
  if (rect) qrOrigin.value = { x: rect.left - bounds.left, y: rect.top - bounds.top, size: rect.width };
}
function openQr(event: Event) {
  if (!qrAvailable.value || merging.value) return;
  const button = event.currentTarget as HTMLElement;
  const circle = button.querySelector('.orbit-circle')?.getBoundingClientRect() ?? button.getBoundingClientRect();
  const bounds = surface.value!.getBoundingClientRect();
  qrOrigin.value = { x: circle.left - bounds.left, y: circle.top - bounds.top, size: circle.width };
}
async function scanned(name: string) {
  qrOrigin.value = undefined;
  await controller.scan(name);
}
function focusInput() {
  if (state.value.step === 3) void nextTick(() => input.value?.focus({ preventScroll: true }));
}
onMounted(() => {
  window.addEventListener('resize', scheduleLayout);
  document.fonts?.addEventListener?.('loadingdone', scheduleLayout);
  void document.fonts?.ready.then(scheduleLayout);
  scheduleLayout();
  window.addEventListener('resize', updateQrOrigin);
  navigator.mediaDevices?.addEventListener?.('devicechange', refreshQrAvailability);
  document.addEventListener('visibilitychange', onVisibilityChange);
  void refreshQrAvailability();
  warmEmailProviders();
  void controller.resume();
});
onBeforeUnmount(() => {
  disposed = true;
  if (layoutFrame !== undefined) cancelAnimationFrame(layoutFrame);
  window.removeEventListener('resize', scheduleLayout);
  document.fonts?.removeEventListener?.('loadingdone', scheduleLayout);
  appleSelection?.abort();
  qrProbeGeneration += 1;
  window.removeEventListener('resize', updateQrOrigin);
  navigator.mediaDevices?.removeEventListener?.('devicechange', refreshQrAvailability);
  document.removeEventListener('visibilitychange', onVisibilityChange);
  controller.dispose();
  if (mergeTimer) clearTimeout(mergeTimer);
});
</script>
<template>
  <section
    ref="surface"
    class="orbit-registration"
    aria-label="Vault registration"
    :data-step="state.step"
    :data-phase="state.phase"
    :data-expanded="expanded"
  >
    <div class="orbit-layout" :inert="qrOrigin ? true : undefined">
      <OrbitScale :balanced="expanded" />
      <OrbitScene :step="state.step" :direction="state.direction" :balanced="expanded" @settled="focusInput">
        <form v-if="state.step === 3" class="orbit-email-form" @submit.prevent="controller.chooseEmail(draft)">
          <label class="orbit-sr-only" for="orbit-email">Enter your email</label>
          <input
            id="orbit-email"
            ref="input"
            v-model="draft"
            class="orbit-email-input"
            type="text"
            inputmode="email"
            autocomplete="email"
            placeholder="Enter your email"
            :aria-invalid="Boolean(state.message)"
            autocapitalize="none"
            spellcheck="false"
          />
          <p v-if="state.message" class="orbit-field-error" role="alert">{{ state.message }}</p>
          <div class="orbit-actions">
            <OrbitControl icon="back" label="Back" motion @activate="controller.back()" /><OrbitControl
              icon="ok"
              label="OK"
              motion
              @activate="controller.chooseEmail(draft)"
            />
          </div>
        </form>
        <template v-else>
          <header class="orbit-heading" :class="{ 'is-identity': state.step !== 1 }">
            <h1>{{ state.step === 1 ? 'MetaSecret' : state.name }}</h1>
            <p
              :class="{ 'is-error': state.phase === 'error' }"
              :role="state.phase === 'error' ? 'alert' : 'status'"
              aria-live="polite"
            >
              {{ subtitle }}
            </p>
          </header>
          <div v-if="state.step === 1" class="orbit-actions">
            <OrbitControl
              icon="apple"
              label="Use Apple email"
              provider
              hide-label
              :disabled="providerBusy || !emailProviderCapabilities.apple"
              @activate="provider('apple')"
            />
            <OrbitControl
              icon="google"
              label="Use Google email"
              provider
              hide-label
              :disabled="providerBusy || !emailProviderCapabilities.google"
              @activate="provider('google')"
            />
            <OrbitControl
              icon="enter"
              label="Enter your email"
              hide-label
              :disabled="providerBusy"
              @activate="controller.edit()"
            />
          </div>
          <p
            v-if="state.step === 1 && (!emailProviderCapabilities.apple || !emailProviderCapabilities.google)"
            class="orbit-capability-note"
          >
            {{
              !emailProviderCapabilities.apple && !emailProviderCapabilities.google
                ? 'Apple and Google sign-in are not configured.'
                : !emailProviderCapabilities.apple
                  ? 'Apple sign-in is not configured.'
                  : 'Google sign-in is not configured.'
            }}
          </p>
          <Transition name="orbit-actions-fade"
            ><div
              v-if="state.step === 2 && !loading && !state.timedCreateError"
              class="orbit-actions"
              :class="{ 'is-error-actions': state.phase === 'error' }"
            >
              <OrbitControl
                icon="create"
                label="Create"
                motion
                :disabled="state.phase !== 'available'"
                @activate="controller.create()"
              />
              <OrbitControl
                icon="join"
                label="Join"
                :motion="state.phase === 'exists'"
                :disabled="state.phase !== 'exists'"
                @activate="controller.join()"
              />
              <OrbitControl icon="enter" label="Enter" motion @activate="controller.edit()" />
            </div>
          </Transition>
          <div
            v-if="state.step === 4 && (state.phase === 'join' || merging)"
            class="orbit-actions orbit-join-actions"
            :class="{ 'is-merging': merging }"
          >
            <OrbitControl
              icon="qr"
              label="QR"
              :disabled="!qrAvailable || merging"
              :title="qrAvailable ? undefined : 'QR scanning requires a phone or tablet with an available camera.'"
              :aria-description="
                qrAvailable ? undefined : 'QR scanning requires a phone or tablet with an available camera.'
              "
              @click="openQr"
            />
            <OrbitControl icon="back" label="Back" :disabled="merging" @activate="controller.back()"
              ><span class="orbit-fusion-mark"><OrbitIcon name="fusion" /></span
            ></OrbitControl>
            <OrbitControl icon="request" label="Request" motion :disabled="merging" @activate="request" />
          </div>
          <OrbitLoader
            v-if="loading && !merging"
            :central="waiting || state.phase === 'syncing'"
            :origin="state.phase === 'creating' ? 'left' : 'center'"
          />
          <div v-if="state.step === 4 && !merging && state.phase !== 'join'" class="orbit-actions orbit-single-action">
            <OrbitControl
              :icon="state.phase === 'syncing' ? 'approved' : 'back'"
              :label="state.phase === 'syncing' ? 'Approved' : 'Back'"
              :disabled="state.phase === 'requesting' || state.phase === 'syncing'"
              @activate="controller.back()"
            />
          </div>
        </template>
      </OrbitScene>
    </div>
    <OrbitQrSheet
      v-if="qrOrigin"
      :origin="qrOrigin"
      :camera-factory="cameraFactory"
      @close="qrOrigin = undefined"
      @scan="scanned"
    />
  </section>
</template>
