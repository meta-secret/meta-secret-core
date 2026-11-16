import { describe, expect, it, vi } from 'vitest';
import { createSecretController } from '@/features/liveboard/secretController';
import type { RevealedValue, SecretPort } from '@/features/liveboard/types';
import { deferred, secret, secretPort, session } from './liveboardFixtures';

function setup(patch: Partial<SecretPort> = {}) {
  const port = secretPort(patch),
    controller = createSecretController(port);
  controller.reconcile({ session: session(), secrets: [secret()] });
  controller.present({ active: true, visibleIds: ['pass-a'], query: '', scrollTop: 0 });
  return { controller, port };
}

describe('Liveboard inline secrets use exact Core instructions', () => {
  it('reveals the exact local Pass only on deliberate Show and exposes inline text', async () => {
    const { controller, port } = setup();
    expect(port.showLocal).not.toHaveBeenCalled();
    await controller.primary('pass-a');
    expect(port.showLocal).toHaveBeenCalledExactlyOnceWith('pass-a');
    expect(controller.state.value.cards['pass-a']).toMatchObject({
      phase: 'revealed',
      value: { kind: 'text', text: 'private value' },
      copied: false,
    });
  });
  it('passes the exact accepted Claim and Pass without submitting another recovery', async () => {
    const { controller, port } = setup();
    controller.reconcile({
      session: session(),
      secrets: [secret('pass-a', { primary: { kind: 'show-recovered', claimId: 'claim-exact' } })],
    });
    await controller.primary('pass-a');
    expect(port.showRecovered).toHaveBeenCalledExactlyOnceWith({ passId: 'pass-a', claimId: 'claim-exact' });
    expect(port.showLocal).not.toHaveBeenCalled();
    expect(port.recover).not.toHaveBeenCalled();
  });
  it('shows a disabled recovery clock and never auto-reveals when approval becomes usable', async () => {
    const { controller, port } = setup();
    controller.reconcile({ session: session(), secrets: [secret('pass-a', { primary: { kind: 'recover' } })] });
    await controller.primary('pass-a');
    expect(port.recover).toHaveBeenCalledExactlyOnceWith('pass-a');
    await controller.primary('pass-a');
    expect(port.recover).toHaveBeenCalledOnce();
    controller.reconcile({
      session: session(),
      secrets: [secret('pass-a', { primary: { kind: 'show-recovered', claimId: 'claim-a' } })],
    });
    expect(port.showRecovered).not.toHaveBeenCalled();
    expect(controller.state.value.cards['pass-a']?.value).toBeNull();
    await controller.primary('pass-a');
    expect(port.showRecovered).toHaveBeenCalledOnce();
  });
  it.each(['waiting', 'blocked'] as const)(
    'honors Core %s even when other input could suggest local Show',
    async (kind) => {
      const { controller, port } = setup();
      controller.reconcile({
        session: session(),
        secrets: [
          secret('pass-a', {
            primary: kind === 'waiting' ? { kind, claimId: 'claim-a' } : { kind },
          }),
        ],
      });
      expect(controller.state.value.cards['pass-a']?.secret.primary.kind).toBe(kind);
      await controller.primary('pass-a');
      expect(port.showLocal).not.toHaveBeenCalled();
      expect(port.showRecovered).not.toHaveBeenCalled();
      expect(port.recover).not.toHaveBeenCalled();
    },
  );
  it('retains a typed seed value and copies its words only on the explicit Copy intent', async () => {
    const words = Array.from({ length: 24 }, (_, index) => `word${index}`);
    const { controller, port } = setup({ showLocal: vi.fn(async () => ({ kind: 'seed' as const, words })) });
    await controller.primary('pass-a');
    expect(controller.state.value.cards['pass-a']?.value).toEqual({ kind: 'seed', words });
    expect(port.copy).not.toHaveBeenCalled();
    await controller.copy('pass-a');
    expect(port.copy).toHaveBeenCalledExactlyOnceWith(words.join(' '));
  });
  it('Hide drops the inline value and Copy state immediately', async () => {
    const { controller, port } = setup();
    await controller.primary('pass-a');
    await controller.copy('pass-a');
    expect(port.copy).toHaveBeenCalledExactlyOnceWith('private value');
    controller.hide('pass-a');
    expect(controller.state.value.cards['pass-a']).toMatchObject({ phase: 'masked', value: null, copied: false });
    await controller.copy('pass-a');
    expect(port.copy).toHaveBeenCalledOnce();
  });
});

describe('Liveboard plaintext lifetime and sync barrier', () => {
  it.each(['hide', 'route', 'filter', 'tombstone', 'member', 'sync', 'pagehide', 'dispose'] as const)(
    'drops a reveal completing after %s, without resurrecting plaintext',
    async (lifecycle) => {
      const reveal = deferred<RevealedValue>();
      const { controller, port } = setup({ showLocal: vi.fn(() => reveal.promise) });
      const pending = controller.primary('pass-a');
      expect(port.showLocal).toHaveBeenCalledExactlyOnceWith('pass-a');
      if (lifecycle === 'hide') controller.hide('pass-a');
      else if (lifecycle === 'route' || lifecycle === 'filter')
        controller.present({
          active: lifecycle !== 'route',
          visibleIds: [],
          query: lifecycle === 'filter' ? 'different' : '',
          scrollTop: 0,
        });
      else if (lifecycle === 'tombstone') controller.reconcile({ session: session(), secrets: [] });
      else if (lifecycle === 'member' || lifecycle === 'sync')
        controller.reconcile({
          session: session({ member: lifecycle !== 'member', ready: lifecycle !== 'sync' }),
          secrets: [secret()],
        });
      else if (lifecycle === 'dispose') controller.dispose();
      else controller.invalidate();
      reveal.resolve({ kind: 'text', text: 'must never appear' });
      await pending;
      expect(controller.state.value.cards['pass-a']?.value ?? null).toBeNull();
      expect(controller.state.value.cards['pass-a']?.copied ?? false).toBe(false);
    },
  );
  it('clears already revealed data before sync pending/error and remains blocked until a fresh ready snapshot', async () => {
    const { controller, port } = setup();
    await controller.primary('pass-a');
    expect(controller.state.value.cards['pass-a']?.value).toEqual({ kind: 'text', text: 'private value' });
    controller.reconcile({ session: session({ ready: false }), secrets: [secret()] });
    expect(controller.state.value.cards['pass-a']?.value).toBeNull();
    expect(controller.state.value.blocked).toBe(true);
    await controller.primary('pass-a');
    await controller.copy('pass-a');
    await controller.remove('pass-a');
    controller.reconcile({ session: session({ ready: false }), secrets: [secret()] });
    expect(port.showLocal).toHaveBeenCalledOnce();
    expect(port.copy).not.toHaveBeenCalled();
    expect(port.deleteSecret).not.toHaveBeenCalled();
    controller.reconcile({ session: session(), secrets: [secret()] });
    await controller.primary('pass-a');
    expect(port.showLocal).toHaveBeenCalledTimes(2);
  });
  it('clears references on a hidden route too and cannot reuse an old session result', async () => {
    const reveal = deferred<RevealedValue>();
    const { controller, port } = setup({ showLocal: vi.fn(() => reveal.promise) });
    const pending = controller.primary('pass-a');
    expect(port.showLocal).toHaveBeenCalledOnce();
    controller.present({ active: false, visibleIds: [], query: '', scrollTop: 0 });
    controller.reconcile({ session: session({ identity: 'session-new' }), secrets: [secret()] });
    controller.present({ active: true, visibleIds: ['pass-a'], query: '', scrollTop: 0 });
    reveal.resolve({ kind: 'text', text: 'old session' });
    await pending;
    expect(controller.state.value.cards['pass-a']?.value).toBeNull();
  });
  it('rejects an accepted-claim result when the exact Core Claim changes during Show', async () => {
    const reveal = deferred<RevealedValue>();
    const { controller, port } = setup({ showRecovered: vi.fn(() => reveal.promise) });
    controller.reconcile({
      session: session(),
      secrets: [secret('pass-a', { primary: { kind: 'show-recovered', claimId: 'old-claim' } })],
    });
    const pending = controller.primary('pass-a');
    expect(port.showRecovered).toHaveBeenCalledOnce();
    controller.reconcile({
      session: session(),
      secrets: [secret('pass-a', { primary: { kind: 'show-recovered', claimId: 'new-claim' } })],
    });
    reveal.resolve({ kind: 'text', text: 'old claim' });
    await pending;
    expect(controller.state.value.cards['pass-a']?.value).toBeNull();
  });
  it('does not restore Copied or value after a clipboard promise completes following Hide', async () => {
    const clipboard = deferred<void>();
    const { controller, port } = setup({ copy: vi.fn(() => clipboard.promise) });
    await controller.primary('pass-a');
    const pending = controller.copy('pass-a');
    expect(port.copy).toHaveBeenCalledExactlyOnceWith('private value');
    controller.hide('pass-a');
    clipboard.resolve();
    await pending;
    expect(controller.state.value.cards['pass-a']).toMatchObject({ value: null, copied: false });
  });
  it('keeps simultaneous cards independent and never uses a display name as the state key', async () => {
    const { controller, port } = setup();
    controller.reconcile({ session: session(), secrets: [secret('pass-a'), secret('pass-b')] });
    controller.present({ active: true, visibleIds: ['pass-a', 'pass-b'], query: '', scrollTop: 0 });
    await controller.primary('pass-a');
    await controller.primary('pass-b');
    controller.hide('pass-a');
    expect(port.showLocal).toHaveBeenNthCalledWith(2, 'pass-b');
    expect(controller.state.value.cards['pass-a']?.value).toBeNull();
    expect(controller.state.value.cards['pass-b']?.value).toEqual({ kind: 'text', text: 'private value' });
  });
});

describe('Liveboard secret deletion stays in the list', () => {
  it('masks the exact card, keeps route/search/scroll, and removes only a canonical tombstoned Pass', async () => {
    const { controller, port } = setup();
    controller.reconcile({ session: session(), secrets: [secret('pass-a'), secret('pass-b')] });
    controller.present({ active: true, visibleIds: ['pass-a', 'pass-b'], query: 'Same', scrollTop: 310 });
    await controller.primary('pass-a');
    await controller.remove('pass-a');
    expect(port.authenticate).toHaveBeenCalledWith({ kind: 'delete-secret', passId: 'pass-a' });
    expect(port.deleteSecret).toHaveBeenCalledExactlyOnceWith('pass-a');
    expect(controller.state.value.cards['pass-a']).toMatchObject({ phase: 'deleting', value: null });
    expect(controller.state.value.presentation).toEqual({
      active: true,
      visibleIds: ['pass-a', 'pass-b'],
      query: 'Same',
      scrollTop: 310,
    });
    controller.reconcile({ session: session(), secrets: [secret('pass-b')] });
    expect(controller.state.value.cards['pass-a']).toBeUndefined();
    expect(controller.state.value.cards['pass-b']).toBeDefined();
    expect(controller.state.value.presentation).toMatchObject({ active: true, query: 'Same', scrollTop: 310 });
  });
  it('cancelled deletion auth sends no write and keeps the exact Secret usable', async () => {
    const { controller, port } = setup({ authenticate: vi.fn(async () => false) });
    await controller.remove('pass-a');
    expect(port.authenticate).toHaveBeenCalledOnce();
    expect(port.deleteSecret).not.toHaveBeenCalled();
    await controller.primary('pass-a');
    expect(port.showLocal).toHaveBeenCalledExactlyOnceWith('pass-a');
  });
  it('a declined checked deletion retains the card and permits an explicit retry', async () => {
    const { controller, port } = setup({
      deleteSecret: vi.fn(async () => ({ operationId: 'delete-a', phase: 'rejected' as const })),
    });
    await controller.remove('pass-a');
    expect(port.deleteSecret).toHaveBeenCalledOnce();
    expect(controller.state.value.cards['pass-a']).toBeDefined();
    expect(port.reportError).toHaveBeenCalledOnce();
    await controller.remove('pass-a');
    expect(port.deleteSecret).toHaveBeenCalledTimes(2);
  });
  it('recreated same-name Pass never inherits old plaintext, pending deletion or stale result', async () => {
    const reveal = deferred<RevealedValue>();
    const { controller, port } = setup({ showLocal: vi.fn(() => reveal.promise) });
    const pending = controller.primary('pass-a');
    expect(port.showLocal).toHaveBeenCalledOnce();
    controller.reconcile({ session: session(), secrets: [secret('new-pass')] });
    controller.present({ active: true, visibleIds: ['new-pass'], query: '', scrollTop: 0 });
    reveal.resolve({ kind: 'text', text: 'deleted old value' });
    await pending;
    expect(controller.state.value.cards['pass-a']).toBeUndefined();
    expect(controller.state.value.cards['new-pass']).toMatchObject({ phase: 'masked', value: null, copied: false });
  });
  it('does not delete the next same-name Pass after auth if the selected exact Pass disappeared', async () => {
    const auth = deferred<boolean>();
    const { controller, port } = setup({ authenticate: vi.fn(() => auth.promise) });
    const pending = controller.remove('pass-a');
    expect(port.authenticate).toHaveBeenCalledOnce();
    controller.reconcile({ session: session(), secrets: [secret('new-pass')] });
    auth.resolve(true);
    await pending;
    expect(port.deleteSecret).not.toHaveBeenCalled();
  });
});
