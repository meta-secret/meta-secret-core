import { onBeforeUnmount, onMounted, ref, type Ref } from 'vue';
import { liveboardMotion } from '@/design/liveboard/motion';

export function useReducedMotion() {
  const reduced = ref(true);
  let query: MediaQueryList | undefined;
  const update = () => {
    reduced.value = query?.matches ?? true;
  };
  onMounted(() => {
    query = matchMedia('(prefers-reduced-motion: reduce)');
    update();
    query.addEventListener('change', update);
  });
  onBeforeUnmount(() => query?.removeEventListener('change', update));
  return reduced;
}
export function useMeasuredHeight(
  element: Ref<HTMLElement | null>,
  name: 'secretHeight' | 'notificationHeight' | 'createHeight',
) {
  let observer: ResizeObserver | undefined;
  let previous = 0;
  let animation: Animation | undefined;
  let frame: number | null = null;
  let disposed = false;
  const reduced = useReducedMotion();
  onMounted(() => {
    const target = element.value;
    if (!target || typeof ResizeObserver === 'undefined') return;
    previous = target.getBoundingClientRect().height;
    const resize = () => {
      frame = null;
      if (disposed || animation?.playState === 'running') return;
      const height = target.getBoundingClientRect().height;
      if (height === previous) return;
      const from = previous;
      previous = height;
      if (reduced.value || !target.animate) return;
      const timeline = liveboardMotion.timelines.transitions[name];
      // Do not observe our own animated height writes. Reconnect after natural
      // layout resumes, so content changes during animation get a fresh measurement.
      observer?.disconnect();
      animation?.cancel();
      const current = target.animate(
        [
          { height: `${from}px`, overflow: name === 'secretHeight' ? 'hidden' : 'visible' },
          { height: `${height}px`, overflow: name === 'secretHeight' ? 'hidden' : 'visible' },
        ],
        { duration: timeline.timing.duration.ms, easing: timeline.timing.easing },
      );
      animation = current;
      void current.finished
        .catch(() => undefined)
        .then(() => {
          if (!disposed && animation === current) observer?.observe(target);
        });
    };
    observer = new ResizeObserver(() => {
      if (disposed || frame !== null || animation?.playState === 'running') return;
      // ResizeObserver delivery is read-only; defer layout writes to a new frame.
      frame = requestAnimationFrame(resize);
    });
    observer.observe(target);
  });
  onBeforeUnmount(() => {
    disposed = true;
    if (frame !== null) cancelAnimationFrame(frame);
    observer?.disconnect();
    animation?.cancel();
  });
}
