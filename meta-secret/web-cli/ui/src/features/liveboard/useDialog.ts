import { nextTick, onBeforeUnmount, watch, type Ref } from 'vue';
import { animateDialog } from './dialogMotion';

export function useDialog(options: {
  element: Ref<HTMLDialogElement | null>;
  open: () => boolean;
  kind: 'create' | 'reset';
}) {
  let revision = 0;
  let effect: Animation | null = null;
  let restore: HTMLElement | null = null;
  let overflow: string | null = null;
  function release() {
    effect?.cancel();
    effect = null;
    options.element.value?.close();
    if (overflow !== null) {
      document.body.style.overflow = overflow;
      overflow = null;
    }
    restore?.focus({ preventScroll: true });
    restore = null;
  }
  watch(
    options.open,
    async (open) => {
      const token = ++revision;
      await nextTick();
      const dialog = options.element.value;
      if (!dialog || token !== revision) return;
      effect?.cancel();
      if (open) {
        if (!dialog.open) {
          restore = document.activeElement instanceof HTMLElement ? document.activeElement : null;
          overflow = document.body.style.overflow;
          document.body.style.overflow = 'hidden';
          dialog.showModal();
        }
        if (options.kind === 'create')
          effect = animateDialog(
            dialog,
            [
              { opacity: 0, transform: 'translateY(22px) scale(.97)' },
              { opacity: 1, transform: 'none' },
            ],
            'createOpen',
          );
        dialog.querySelector<HTMLElement>('[autofocus]')?.focus({ preventScroll: true });
      } else if (dialog.open) {
        if (options.kind === 'create')
          effect = animateDialog(
            dialog,
            [
              { opacity: 1, transform: 'none' },
              { opacity: 0, transform: 'translateY(18px) scale(.98)' },
            ],
            'createClose',
          );
        try {
          await effect?.finished;
        } catch {
          return;
        }
        if (token === revision) release();
      }
    },
    { immediate: true },
  );
  onBeforeUnmount(() => {
    revision++;
    release();
  });
}
