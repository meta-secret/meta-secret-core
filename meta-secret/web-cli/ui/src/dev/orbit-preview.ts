import { createApp, defineComponent, h, ref } from 'vue';
import OrbitRegistration from '@/components/registration/OrbitRegistration.vue';
import type { RegistrationPort } from '@/features/registration/registrationController';
import type { RegistrationSnapshot } from '@/features/registration/registrationState';
import type { CameraScannerFactory } from '@/features/registration/cameraScanner';

if (!import.meta.env.DEV) throw new Error('Orbit development preview is not available in production.');

const Preview = defineComponent({
  setup() {
    const scene = ref<RegistrationSnapshot['kind']>('local');
    const key = ref(0);
    const toolbar = ref(true);
    const taken = ref(true);
    const ready = ref(false);
    const result = ref('accepted');
    let canonical: RegistrationSnapshot = { kind: 'local', name: '' };
    let notify: (() => Promise<void>) | undefined;
    const name = 'dmitry@apple.example';
    const port: RegistrationPort = {
      snapshot: () => canonical,
      prepare: async (value) => {
        canonical = { kind: taken.value ? 'exists' : 'available', name: value };
        return canonical;
      },
      submit: async () => {
        canonical = { kind: canonical.kind === 'available' ? 'member' : 'pending', name: canonical.name };
        return canonical;
      },
      refresh: async () => canonical,
      sync: async () => undefined,
      reset: async () => {
        canonical = { kind: 'local', name: '' };
      },
      observe: (_name, callback) => {
        notify = callback;
      },
      stopObserving: () => {
        notify = undefined;
      },
    };
    const camera: CameraScannerFactory = (_video, _scan, error) => ({
      start: async () => {
        error('Development preview · camera simulation');
      },
      stop: () => undefined,
    });
    const reset = () => {
      canonical = { kind: scene.value, name: scene.value === 'local' ? '' : name };
      ready.value = false;
      key.value += 1;
    };
    const decide = async () => {
      canonical = { kind: result.value === 'accepted' ? 'member' : 'declined', name: canonical.name || name };
      await notify?.();
    };
    return () =>
      h('div', [
        h(OrbitRegistration, {
          key: key.value,
          port,
          cameraFactory: camera,
          onReady: () => {
            ready.value = true;
          },
        }),
        h(
          'aside',
          {
            style:
              'position:fixed;top:12px;right:12px;z-index:30;padding:10px;border:1px solid #69716d;border-radius:10px;background:#232626e6;color:#eee;font:12px sans-serif;max-width:95vw;display:flex;gap:10px;align-items:center;flex-wrap:wrap',
          },
          toolbar.value
            ? [
                h('span', 'Development only · simulated Core / camera'),
                h(
                  'select',
                  {
                    'aria-label': 'Preview scene',
                    value: scene.value,
                    onChange: (e: Event) => {
                      scene.value = (e.target as HTMLSelectElement).value as RegistrationSnapshot['kind'];
                      reset();
                    },
                  },
                  ['local', 'available', 'exists', 'pending', 'declined'].map((v) => h('option', { value: v }, v)),
                ),
                h('label', [
                  h('input', {
                    type: 'checkbox',
                    checked: taken.value,
                    onChange: () => {
                      taken.value = !taken.value;
                    },
                  }),
                  ' Name taken',
                ]),
                h(
                  'select',
                  {
                    'aria-label': 'Approval outcome',
                    value: result.value,
                    onChange: (e: Event) => {
                      result.value = (e.target as HTMLSelectElement).value;
                    },
                  },
                  ['accepted', 'declined'].map((v) => h('option', { value: v }, v)),
                ),
                h('button', { onClick: decide }, 'Send decision'),
                h('button', { onClick: reset }, 'Start over'),
                h(
                  'button',
                  {
                    onClick: () => {
                      toolbar.value = false;
                    },
                  },
                  'Hide preview controls',
                ),
              ]
            : [
                h(
                  'button',
                  {
                    onClick: () => {
                      toolbar.value = true;
                    },
                  },
                  'Preview controls',
                ),
              ],
        ),
        ready.value
          ? h(
              'div',
              {
                role: 'status',
                style:
                  'position:fixed;bottom:20px;left:50%;transform:translateX(-50%);z-index:30;color:#eee;font:12px sans-serif',
              },
              'Preview: Core readiness reached · real app opens existing Vault',
            )
          : null,
      ]);
  },
});
createApp(Preview).mount('#orbit-preview');
