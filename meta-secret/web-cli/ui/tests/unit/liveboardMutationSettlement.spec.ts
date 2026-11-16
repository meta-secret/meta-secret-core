import { describe, expect, it, vi } from 'vitest';
import { createSecretController } from '@/features/liveboard/secretController';
import type { OperationReceipt, RevealedValue, SecretPort } from '@/features/liveboard/types';
import { deferred, secret, secretPort, session } from './liveboardFixtures';

const kinds = ['recover', 'delete'] as const;
type MutationKind = (typeof kinds)[number];

function setup(kind: MutationKind, patch: Partial<SecretPort> = {}) {
  const port = secretPort({
    recover: vi.fn(async (id) => ({ operationId: `claim-${id}`, phase: 'unknown' as const })),
    deleteSecret: vi.fn(async (id) => ({ operationId: `delete-${id}`, phase: 'unknown' as const })),
    ...patch,
  });
  const controller = createSecretController(port);
  const secrets = [
    secret('pass-a', { primary: { kind: kind === 'recover' ? 'recover' : 'show-local' } }),
    secret('pending-recover', { primary: { kind: 'recover' } }),
    secret('pending-delete'),
    secret('held'),
  ];
  const snapshot = { session: session(), secrets };
  controller.reconcile(snapshot);
  controller.present({
    active: true,
    visibleIds: secrets.map((item) => item.passId),
    query: 'Same',
    scrollTop: 237,
  });
  const submit = () => (kind === 'recover' ? controller.primary('pass-a') : controller.remove('pass-a'));
  return { controller, port, snapshot, submit };
}

describe('Exact canonical Declined settles only its current local mutation', () => {
  it.each(kinds)('releases unchanged %s while preserving other Unknown latches and held plaintext', async (kind) => {
    const { controller, port, snapshot, submit } = setup(kind);
    if (kind === 'delete') {
      await controller.primary('pass-a');
      await controller.copy('pass-a');
    }
    await controller.primary('held');
    await controller.copy('held');
    const card = controller.state.value.cards['pass-a'];
    const held = controller.state.value.cards.held;
    await controller.primary('pending-recover');
    await controller.remove('pending-delete');
    await submit();
    controller.reconcile(snapshot);
    await submit();
    expect(kind === 'recover' ? port.recover : port.deleteSecret).toHaveBeenCalledTimes(2);
    expect(card).toMatchObject({ phase: kind === 'recover' ? 'waiting' : 'deleting', value: null, copied: false });

    controller.settleMutation({ passId: 'pass-a', kind });

    expect(controller.state.value.cards['pass-a']).toBe(card);
    expect(card).toMatchObject({
      secret: snapshot.secrets[0],
      phase: 'masked',
      value: null,
      copied: false,
      conceal: false,
    });
    expect(controller.state.value.cards.held).toBe(held);
    expect(held).toMatchObject({ phase: 'revealed', value: { kind: 'text', text: 'private value' }, copied: true });
    expect(controller.state.value.cards['pending-recover']?.phase).toBe('waiting');
    expect(controller.state.value.cards['pending-delete']?.phase).toBe('deleting');
    expect(controller.state.value.presentation).toEqual({
      active: true,
      visibleIds: ['pass-a', 'pending-recover', 'pending-delete', 'held'],
      query: 'Same',
      scrollTop: 237,
    });
    expect(controller.state.value.blocked).toBe(false);
    await controller.primary('pending-recover');
    await controller.remove('pending-delete');
    expect(port.recover).toHaveBeenCalledTimes(kind === 'recover' ? 2 : 1);
    expect(port.deleteSecret).toHaveBeenCalledTimes(kind === 'delete' ? 2 : 1);
    await submit();
    const selected = kind === 'recover' ? port.recover : port.deleteSecret;
    expect(selected).toHaveBeenCalledTimes(3);
    expect(selected).toHaveBeenLastCalledWith('pass-a');
    expect(port.reportError).not.toHaveBeenCalled();
    controller.dispose();
  });

  it.each(kinds)('ignores the wrong kind for a pending %s', async (kind) => {
    const { controller, port, submit } = setup(kind);
    await submit();
    await controller.primary('held');
    await controller.copy('held');
    const before = JSON.stringify(controller.state.value);

    controller.settleMutation({ passId: 'pass-a', kind: kind === 'recover' ? 'delete' : 'recover' });

    expect(JSON.stringify(controller.state.value)).toBe(before);
    await submit();
    expect(kind === 'recover' ? port.recover : port.deleteSecret).toHaveBeenCalledExactlyOnceWith('pass-a');
    controller.dispose();
  });

  it('ignores absent and tombstoned same-name Passes without releasing a different pending Pass', async () => {
    const { controller, port, snapshot } = setup('recover');
    await controller.primary('pass-a');
    await controller.primary('pending-recover');
    await controller.primary('held');
    await controller.copy('held');
    controller.reconcile({ ...snapshot, secrets: snapshot.secrets.filter((item) => item.passId !== 'pass-a') });
    const before = JSON.stringify(controller.state.value);

    controller.settleMutation({ passId: 'missing', kind: 'recover' });
    controller.settleMutation({ passId: 'pass-a', kind: 'recover' });

    expect(JSON.stringify(controller.state.value)).toBe(before);
    expect(controller.state.value.cards['pass-a']).toBeUndefined();
    await controller.primary('pending-recover');
    expect(port.recover).toHaveBeenCalledTimes(2);
    expect(port.recover).toHaveBeenNthCalledWith(1, 'pass-a');
    expect(port.recover).toHaveBeenNthCalledWith(2, 'pending-recover');
    controller.dispose();
  });

  it('does nothing on repeat without a current mutation and preserves a later deliberate Show', async () => {
    const { controller, port, submit } = setup('delete');
    await submit();
    controller.settleMutation({ passId: 'pass-a', kind: 'delete' });
    await controller.primary('pass-a');
    await controller.copy('pass-a');
    const before = JSON.stringify(controller.state.value);

    controller.settleMutation({ passId: 'pass-a', kind: 'delete' });

    expect(JSON.stringify(controller.state.value)).toBe(before);
    expect(port.deleteSecret).toHaveBeenCalledExactlyOnceWith('pass-a');
    expect(port.showLocal).toHaveBeenCalledExactlyOnceWith('pass-a');
    expect(port.copy).toHaveBeenCalledExactlyOnceWith('private value');
    controller.dispose();
  });

  it('remains inert after disposal', async () => {
    const { controller, port, submit } = setup('recover');
    await submit();
    controller.dispose();
    const before = JSON.stringify(controller.state.value);

    controller.settleMutation({ passId: 'pass-a', kind: 'recover' });

    expect(JSON.stringify(controller.state.value)).toBe(before);
    await submit();
    expect(port.recover).toHaveBeenCalledExactlyOnceWith('pass-a');
    expect(port.reportError).not.toHaveBeenCalled();
  });

  it.each(kinds)('an old deferred %s receipt/finally cannot erase the new retry owner', async (kind) => {
    const old = deferred<OperationReceipt>();
    const retry = deferred<OperationReceipt>();
    const mutate = vi
      .fn<SecretPort['recover']>()
      .mockImplementationOnce(() => old.promise)
      .mockImplementationOnce(() => retry.promise);
    const { controller, port, snapshot, submit } = setup(
      kind,
      kind === 'recover' ? { recover: mutate } : { deleteSecret: mutate },
    );
    const first = submit();
    await Promise.resolve();
    expect(mutate).toHaveBeenCalledExactlyOnceWith('pass-a');
    const held = controller.state.value.cards['pass-a'];
    controller.reconcile(snapshot);

    controller.settleMutation({ passId: 'pass-a', kind });

    const second = submit();
    await Promise.resolve();
    expect(mutate).toHaveBeenCalledTimes(2);
    old.resolve({ operationId: 'old-exact-operation', phase: 'rejected' });
    await first;
    expect(controller.state.value.cards['pass-a']).toBe(held);
    expect(held).toMatchObject({ phase: kind === 'recover' ? 'waiting' : 'deleting', value: null, copied: false });
    await submit();
    expect(mutate).toHaveBeenCalledTimes(2);
    retry.resolve({ operationId: 'retry-exact-operation', phase: 'unknown' });
    await second;
    await submit();
    expect(mutate).toHaveBeenCalledTimes(2);
    expect(mutate).toHaveBeenNthCalledWith(2, 'pass-a');
    expect(port.reportError).not.toHaveBeenCalled();
    controller.dispose();
  });

  it.each(['show', 'copy'] as const)(
    'never restores stale %s completion over a later deliberate Show',
    async (action) => {
      const reveal = deferred<RevealedValue>();
      const clipboard = deferred<void>();
      const showLocal = vi
        .fn<SecretPort['showLocal']>()
        .mockImplementationOnce(() =>
          action === 'show' ? reveal.promise : Promise.resolve({ kind: 'text', text: 'old value' }),
        )
        .mockResolvedValue({ kind: 'text', text: 'new deliberate value' });
      const { controller, port, submit } = setup('delete', {
        showLocal,
        copy: vi.fn(() => clipboard.promise),
      });
      let pending: Promise<void>;
      if (action === 'show') pending = controller.primary('pass-a');
      else {
        await controller.primary('pass-a');
        pending = controller.copy('pass-a');
      }
      await submit();
      const held = controller.state.value.cards['pass-a'];
      expect(held).toMatchObject({ phase: 'deleting', value: null, copied: false });

      controller.settleMutation({ passId: 'pass-a', kind: 'delete' });

      expect(held).toMatchObject({ phase: 'masked', value: null, copied: false, conceal: false });
      await controller.primary('pass-a');
      reveal.resolve({ kind: 'text', text: 'stale value' });
      clipboard.resolve();
      await pending;
      expect(controller.state.value.cards['pass-a']).toBe(held);
      expect(held).toMatchObject({
        phase: 'revealed',
        value: { kind: 'text', text: 'new deliberate value' },
        copied: false,
      });
      expect(showLocal).toHaveBeenCalledTimes(2);
      expect(showLocal).toHaveBeenNthCalledWith(2, 'pass-a');
      expect(port.deleteSecret).toHaveBeenCalledExactlyOnceWith('pass-a');
      controller.dispose();
    },
  );
});
