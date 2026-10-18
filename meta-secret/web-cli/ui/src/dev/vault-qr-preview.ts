import { createApp, defineComponent, h, ref, watch } from 'vue';
import VaultNameQrDialog from '@/components/vault/VaultNameQrDialog.vue';
import { Button } from '@/components/ui/button';
import '@/index.css';

if (!import.meta.env.DEV) throw new Error('Vault QR development preview is not available in production.');

const Preview = defineComponent({
  setup() {
    const name = ref('Dmitrii@Sample.example');
    const open = ref(false);
    watch(
      name,
      () => {
        open.value = false;
      },
      { flush: 'sync' },
    );
    return () =>
      h('main', { class: 'min-h-dvh bg-background p-6 text-foreground' }, [
        h('p', { class: 'mb-4 text-sm text-muted-foreground' }, 'Development only · simulated Core'),
        h('label', { class: 'mb-4 flex max-w-sm flex-col gap-2 text-sm' }, [
          'Sample Vault name',
          h('input', {
            value: name.value,
            class: 'rounded-md border border-input bg-transparent p-2',
            onInput: (event: Event) => {
              name.value = (event.target as HTMLInputElement).value;
            },
          }),
        ]),
        h(
          Button,
          {
            disabled: !name.value,
            onClick: () => {
              open.value = true;
            },
          },
          () => 'Add device',
        ),
        h(VaultNameQrDialog, {
          open: open.value,
          vaultName: name.value,
          onClose: () => {
            open.value = false;
          },
        }),
      ]);
  },
});
createApp(Preview).mount('#vault-qr-preview');
