import { onBeforeUnmount, onMounted, watch, type Ref } from 'vue';
import { liveboardCreateFit } from '@/design/liveboard/runtime';

/** The reviewed shared fit owns every breakpoint/inset; CSS retains vertical centering. */
export function useCreateFit(options: { element: Ref<HTMLDialogElement | null>; open: () => boolean }) {
  let visualViewport: VisualViewport | null = null;
  function fit() {
    const dialog = options.element.value;
    if (!dialog || !options.open()) return;
    const frame = dialog.closest('.ms-liveboard')?.getBoundingClientRect() ?? { left: 0, width: innerWidth };
    const bounds = liveboardCreateFit(frame, {
      width: innerWidth,
      height: innerHeight,
      visualHeight: visualViewport?.height,
    });
    dialog.style.width = `${bounds.width}px`;
    dialog.style.setProperty('--create-max', `${bounds.maxHeight}px`);
    dialog.style.left = `${bounds.left}px`;
    dialog.style.right = bounds.right === null ? 'auto' : `${bounds.right}px`;
  }
  watch(options.open, fit, { flush: 'post' });
  onMounted(() => {
    visualViewport = window.visualViewport;
    window.addEventListener('resize', fit);
    visualViewport?.addEventListener('resize', fit);
    fit();
  });
  onBeforeUnmount(() => {
    window.removeEventListener('resize', fit);
    visualViewport?.removeEventListener('resize', fit);
    visualViewport = null;
  });
}
