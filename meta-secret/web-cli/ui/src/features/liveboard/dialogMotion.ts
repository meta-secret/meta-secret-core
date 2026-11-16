import { liveboardMotion } from '@/design/liveboard/motion';

type Transition = keyof typeof liveboardMotion.timelines.transitions;
export function animateDialog(element: Element, frames: Keyframe[], name: Transition) {
  if (!element.animate || matchMedia('(prefers-reduced-motion: reduce)').matches) return null;
  const timing = liveboardMotion.timelines.transitions[name].timing;
  const duration = timing.duration.kind === 'fixed' ? timing.duration.ms : 0;
  const delay = timing.delay.kind === 'fixed' ? timing.delay.ms : 0;
  return element.animate(frames, { duration, delay, easing: timing.easing, fill: timing.fill });
}
export function outsideDialog(event: MouseEvent, dialog: HTMLDialogElement) {
  const rect = dialog.getBoundingClientRect();
  return (
    event.target === dialog &&
    (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom)
  );
}
