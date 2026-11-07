import { describe, expect, it, vi } from 'vitest';
import { createCreateController } from '@/features/liveboard/createController';
import type { CreatePort, CreateReceipt } from '@/features/liveboard/types';
import { createPort, deferred, session } from './liveboardFixtures';

function setup(patch: Partial<CreatePort> = {}) {
  const port = createPort(patch),
    controller = createCreateController(port);
  controller.reconcile(session());
  controller.open();
  controller.change({ name: 'Account', value: 'private draft' });
  return { controller, port };
}

describe('Liveboard Create transient draft', () => {
  it('opens an editable form without creating or inventing Web authentication', () => {
    const { controller, port } = setup();
    expect(controller.state.value).toMatchObject({
      open: true,
      phase: 'idle',
      draft: { name: 'Account', value: 'private draft' },
    });
    expect(port.create).not.toHaveBeenCalled();
  });
  it('preserves both mode drafts and all24 words across mode/count switches', () => {
    const { controller } = setup(),
      words = Array.from({ length: 24 }, (_, i) => `word${i}`);
    controller.change({ mode: 'seed', wordCount: 24, words });
    controller.change({ wordCount: 12 });
    controller.change({ mode: 'secret' });
    controller.change({ mode: 'seed', wordCount: 24 });
    expect(controller.state.value.draft).toEqual({
      mode: 'seed',
      name: 'Account',
      value: 'private draft',
      wordCount: 24,
      words,
    });
  });
  it('keeps ordinary validation in the form and never submits empty active fields', async () => {
    const { controller, port } = setup();
    controller.change({ name: ' ' });
    expect(controller.state.value.open).toBe(true);
    await controller.submit();
    expect(port.create).not.toHaveBeenCalled();
    controller.change({ name: 'Seed', mode: 'seed', words: Array<string>(24).fill('') });
    await controller.submit();
    expect(port.create).not.toHaveBeenCalled();
  });
  it('pastes a whole12 word phrase using the explicit clipboard intent', async () => {
    const { controller, port } = setup();
    controller.change({ mode: 'seed' });
    await controller.paste();
    expect(port.readClipboard).toHaveBeenCalledOnce();
    expect(controller.state.value.draft.wordCount).toBe(12);
    expect(controller.state.value.draft.words.slice(0, 12)).toEqual(
      'one two three four five six seven eight nine ten eleven twelve'.split(' '),
    );
    expect(port.create).not.toHaveBeenCalled();
  });
  it('accepts whitespace-separated24 word manual paste without reading the clipboard', () => {
    const { controller, port } = setup(),
      words = Array.from({ length: 24 }, (_, i) => `word${i}`);
    controller.change({ mode: 'seed' });
    controller.pasteText(`  ${words.join('\n ')}  `);
    expect(controller.state.value.draft).toMatchObject({ wordCount: 24, words });
    expect(port.readClipboard).not.toHaveBeenCalled();
  });
  it('clipboard denial keeps the manual draft and exposes a nonsensitive paste hint', async () => {
    const { controller, port } = setup({
      readClipboard: vi.fn(async () => {
        throw new DOMException('Denied', 'NotAllowedError');
      }),
    });
    controller.change({ mode: 'seed' });
    await controller.paste();
    expect(port.readClipboard).toHaveBeenCalledOnce();
    expect(controller.state.value.pasteHint).toBeTruthy();
    expect(controller.state.value.draft.value).toBe('private draft');
    expect(controller.state.value.open).toBe(true);
  });
  it('a late clipboard response cannot populate a closed and reopened form', async () => {
    const clipboard = deferred<string>();
    const { controller, port } = setup({ readClipboard: vi.fn(() => clipboard.promise) });
    controller.change({ mode: 'seed' });
    const pending = controller.paste();
    expect(port.readClipboard).toHaveBeenCalledOnce();
    controller.cancel();
    controller.open();
    controller.change({ name: 'New form', mode: 'seed' });
    clipboard.resolve('old old old old old old old old old old old old');
    await pending;
    expect(controller.state.value.draft.name).toBe('New form');
    expect(controller.state.value.draft.words.every((word) => word === '')).toBe(true);
  });
  it('idle Cancel clears every sensitive field before another open', () => {
    const { controller } = setup();
    expect(controller.state.value.draft.value).toBe('private draft');
    controller.cancel();
    expect(controller.state.value.open).toBe(false);
    expect(JSON.stringify(controller.state.value)).not.toContain('private draft');
    controller.open();
    expect(controller.state.value.draft).toMatchObject({ name: '', value: '' });
  });
});

describe('Liveboard checked Create outcomes', () => {
  it('serializes actual work and never closes on a locally submitted receipt', async () => {
    const work = deferred<CreateReceipt>(),
      { controller, port } = setup({ create: vi.fn(() => work.promise) });
    const pending = controller.submit();
    await controller.submit();
    expect(port.create).toHaveBeenCalledOnce();
    expect(controller.state.value.phase).toBe('saving');
    controller.cancel();
    controller.change({ value: 'must not replace submitted draft' });
    expect(controller.state.value.open).toBe(true);
    expect(controller.state.value.draft.value).toBe('private draft');
    work.resolve({ operationId: 'exact-create', phase: 'submitted' });
    await pending;
    expect(controller.state.value.open).toBe(true);
    expect(controller.state.value.receipt?.operationId).toBe('exact-create');
  });
  it('unknown persisted outcome reconciles only its exact receipt and never performs a second write', async () => {
    const { controller, port } = setup({
      create: vi.fn(async () => ({ operationId: 'exact-create', phase: 'unknown' as const })),
    });
    await controller.submit();
    expect(port.create).toHaveBeenCalledOnce();
    expect(controller.state.value).toMatchObject({ phase: 'reconciling', draft: { value: 'private draft' } });
    await controller.submit();
    await controller.refreshReceipt();
    expect(port.create).toHaveBeenCalledOnce();
    expect(port.reconcile).toHaveBeenCalledExactlyOnceWith('exact-create');
  });
  it('ignores a different receipt even if its display name would match, then closes only on exact canonical success', async () => {
    const { controller, port } = setup();
    await controller.submit();
    expect(port.create).toHaveBeenCalledOnce();
    controller.applyReceipt({ operationId: 'another-create', phase: 'confirmed', passId: 'same-name-pass' });
    expect(controller.state.value.open).toBe(true);
    controller.applyReceipt({ operationId: 'create-a', phase: 'confirmed', passId: 'canonical-pass' });
    expect(controller.state.value).toMatchObject({ open: false, phase: 'idle', draft: { name: '', value: '' } });
  });
  it('a known rejected receipt preserves the same draft for deliberate retry and reports through04', async () => {
    const { controller, port } = setup({
      create: vi.fn(async () => ({ operationId: 'failed-create', phase: 'rejected' as const })),
    });
    await controller.submit();
    expect(port.create).toHaveBeenCalledOnce();
    expect(controller.state.value).toMatchObject({
      open: true,
      phase: 'idle',
      draft: { name: 'Account', value: 'private draft' },
    });
    expect(port.reportError).toHaveBeenCalledOnce();
    await controller.submit();
    expect(port.create).toHaveBeenCalledTimes(2);
  });
  it('an unexpected transport exception cannot enable blind retry when no exact outcome was established', async () => {
    const { controller, port } = setup({
      create: vi.fn(async () => {
        throw new TypeError('Failed to fetch');
      }),
    });
    await controller.submit();
    expect(port.create).toHaveBeenCalledOnce();
    expect(port.reportError).toHaveBeenCalledOnce();
    expect(controller.state.value).toMatchObject({
      open: true,
      phase: 'reconciling',
      draft: { value: 'private draft' },
    });
    await controller.submit();
    expect(port.create).toHaveBeenCalledOnce();
    expect(port.reconcile).not.toHaveBeenCalled();
  });
  it.each(['member', 'session', 'dispose'] as const)(
    'clears a busy draft on %s loss and fences its late completion',
    async (lifecycle) => {
      const work = deferred<CreateReceipt>(),
        { controller, port } = setup({ create: vi.fn(() => work.promise) });
      const pending = controller.submit();
      expect(port.create).toHaveBeenCalledOnce();
      if (lifecycle === 'dispose') controller.dispose();
      else
        controller.reconcile(
          session({ member: lifecycle !== 'member', identity: lifecycle === 'session' ? 'session-b' : 'session-a' }),
        );
      work.resolve({ operationId: 'old-create', phase: 'confirmed', passId: 'old-pass' });
      await pending;
      expect(controller.state.value.open).toBe(false);
      expect(controller.state.value.draft.value).toBe('');
      expect(controller.state.value.receipt).toBeNull();
    },
  );
  it('blocks submission while canonical synchronization is not ready without dismissing the draft', async () => {
    const { controller, port } = setup();
    expect(controller.state.value.draft.value).toBe('private draft');
    controller.reconcile(session({ ready: false }));
    await controller.submit();
    expect(port.create).not.toHaveBeenCalled();
    expect(controller.state.value.draft.value).toBe('private draft');
    controller.reconcile(session());
    await controller.submit();
    expect(port.create).toHaveBeenCalledOnce();
  });
});
