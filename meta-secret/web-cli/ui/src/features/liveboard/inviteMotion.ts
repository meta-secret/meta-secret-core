import { liveboardMotion } from '@/design/liveboard/motion';
import { animateDialog } from './dialogMotion';

/** The popup's top-right remains the plus button's top-right at every frame. */
export function createInviteMotion(elements: { dialog: HTMLDialogElement; body: HTMLElement; anchor: HTMLElement }) {
  let effects: (Animation | null)[] = [];
  let revision = 0;
  function cancel() {
    revision++;
    effects.forEach((effect) => effect?.cancel());
    effects = [];
  }
  function fit(makeRoom = false) {
    const layout = liveboardMotion.timelines.inviteLayout;
    let origin = elements.anchor.getBoundingClientRect();
    if (makeRoom && innerHeight - origin.top < layout.placement.minimumSpaceBelow) {
      window.scrollBy({ top: origin.top - layout.placement.scrollTopInset, behavior: 'instant' });
      origin = elements.anchor.getBoundingClientRect();
    }
    const root = elements.anchor.closest('.ms-liveboard')?.getBoundingClientRect();
    const width = Math.min(
      layout.maxSize.width,
      origin.right - Math.max(layout.placement.edgeInset, (root?.left ?? 0) + layout.placement.edgeInset),
    );
    const height = Math.min(layout.maxSize.height, innerHeight - origin.top - layout.placement.edgeInset);
    Object.assign(elements.dialog.style, {
      top: `${origin.top}px`,
      right: `${innerWidth - origin.right}px`,
      width: `${width}px`,
      height: `${height}px`,
    });
    elements.dialog.style.setProperty('--invite-width', `${width}px`);
    elements.dialog.style.setProperty('--invite-height', `${height}px`);
    elements.dialog.style.setProperty(
      '--code-size',
      `${Math.min(layout.code.maxSize, width - layout.code.widthInset, Math.max(layout.code.minSize, height - layout.code.heightInset))}px`,
    );
    return { origin, width, height };
  }
  async function open() {
    cancel();
    const token = revision,
      { origin, width, height } = fit(true),
      radii = liveboardMotion.timelines.inviteLayout.radii;
    elements.dialog.dataset.phase = 'opening';
    if (!elements.dialog.open) elements.dialog.showModal();
    elements.dialog.querySelector<HTMLElement>('button')?.focus({ preventScroll: true });
    effects = [
      animateDialog(
        elements.dialog,
        [
          { width: `${origin.width}px`, height: `${origin.height}px`, borderRadius: `${radii.collapsed}px` },
          { width: `${width}px`, height: `${height}px`, borderRadius: `${radii.expanded}px` },
        ],
        'inviteOpen',
      ),
      animateDialog(
        elements.body,
        [
          { opacity: 0, transform: 'translateY(-5px)' },
          { opacity: 1, transform: 'none' },
        ],
        'inviteContent',
      ),
    ];
    try {
      await effects[0]?.finished;
    } catch {
      return;
    }
    if (token === revision) elements.dialog.dataset.phase = 'open';
  }
  async function close() {
    const style = getComputedStyle(elements.dialog),
      opacity = getComputedStyle(elements.body).opacity;
    const from = { width: style.width, height: style.height, borderRadius: style.borderRadius };
    cancel();
    const token = revision,
      origin = elements.anchor.getBoundingClientRect();
    elements.dialog.dataset.phase = 'closing';
    effects = [
      animateDialog(
        elements.dialog,
        [
          from,
          {
            width: `${origin.width}px`,
            height: `${origin.height}px`,
            borderRadius: `${liveboardMotion.timelines.inviteLayout.radii.collapsed}px`,
          },
        ],
        'inviteClose',
      ),
      animateDialog(elements.body, [{ opacity }, { opacity: 0 }], 'inviteContentClose'),
    ];
    try {
      await effects[0]?.finished;
    } catch {
      return;
    }
    if (token !== revision) return;
    elements.dialog.close();
    cancel();
    delete elements.dialog.dataset.phase;
    elements.anchor.focus({ preventScroll: true });
  }
  return { open, close, fit, cancel };
}
