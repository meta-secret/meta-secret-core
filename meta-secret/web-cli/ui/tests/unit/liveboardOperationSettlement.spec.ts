import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushPromises } from '@vue/test-utils';
import { createCoreAdapter } from '@/features/liveboard/coreAdapter';
import { createLiveboardController } from '@/features/liveboard/controller';
import type { CoreOperation } from '@/features/liveboard/coreTypes';
import {
  checked,
  contextFixture,
  coreState,
  deferred,
  device,
  pass as fixturePass,
  platformFixture,
  producer,
  requestId,
  secondRequestId,
} from './liveboardCoreFixtures';

const kinds = ['recover', 'delete'] as const;
type Kind = (typeof kinds)[number];
const controllers: ReturnType<typeof createLiveboardController>[] = [];
afterEach(() => {
  for (const controller of controllers.splice(0)) controller.dispose();
});

function material(kind: Kind) {
  return [
    {
      secretId: 'pass-a',
      primaryAction: kind === 'recover' ? 'recover' : 'showLocal',
      canDelete: true,
      readiness: 'ready',
    },
    { secretId: 'pass-b', primaryAction: 'recover', canDelete: true, readiness: 'ready' },
  ];
}
function terminal(kind: Kind, patch: Partial<CoreOperation> = {}): CoreOperation {
  return {
    kind: kind === 'recover' ? 'recover' : 'deleteSecret',
    requestId: kind === 'recover' ? 'claim-second' : requestId,
    secretId: 'pass-a',
    requesterId: 'self',
    clientStatus: 'declined',
    outcome: 'declined',
    eligiblePeerIds: ['peer-a'],
    canApprove: false,
    canDecline: false,
    responderId: 'peer-a',
    resolutionSource: kind === 'recover' ? 'verified' : 'canonical',
    readiness: kind === 'recover' ? 'waiting' : 'settled',
    ...patch,
  };
}
function deleteReceipt(retry = false) {
  // Replace only UUID metadata; raw numeric source tokens never pass through Number/JSON.parse.
  return producer({ curr: retry ? '18446744073709551613' : '18446744073709551615' }).replace(
    `"requestId":"${requestId}"`,
    `"requestId":"${retry ? secondRequestId : requestId}"`,
  );
}
function preparedContext(kind: Kind) {
  const context = contextFixture(
    JSON.stringify(coreState({ storageMode: 'shares', secrets: material(kind), operations: [] })),
  );
  const users = vi.mocked(context.vaultData.users).getMockImplementation()!;
  const members = vi.mocked(context.vaultData.members).getMockImplementation()!;
  // Three matching logical members allow recovery from an encrypted local share.
  vi.mocked(context.vaultData.users).mockImplementation(() => {
    const third = users()[1]!;
    third.user_data().device.device_id = device('peer-b');
    return [...users(), third];
  });
  vi.mocked(context.vaultData.members).mockImplementation(() => {
    const third = members()[1]!;
    third.user_data.device.device_id = device('peer-b');
    return [...members(), third];
  });
  vi.mocked(context.manager.recover_checked).mockImplementation(async (pass) =>
    checked({
      secretId: pass.id_str(),
      claimId: pass.id_str() === 'pass-a' ? 'claim-second' : 'claim-other',
      phase: 'unknown',
    }),
  );
  vi.mocked(context.manager.delete_secret_checked).mockResolvedValue(deleteReceipt());
  vi.mocked(context.manager.reconcile_producer_receipt).mockResolvedValue(deleteReceipt());
  return context;
}
function armRetry(context: ReturnType<typeof preparedContext>, kind: Kind) {
  if (kind === 'recover')
    vi.mocked(context.manager.recover_checked).mockResolvedValueOnce(
      checked({ claimId: 'claim-first', phase: 'unknown' }),
    );
  else {
    vi.mocked(context.manager.delete_secret_checked).mockResolvedValueOnce(deleteReceipt(true));
    vi.mocked(context.manager.reconcile_producer_receipt).mockResolvedValue(deleteReceipt(true));
  }
}
async function setup(kind: Kind) {
  let context = preparedContext(kind);
  const platform = platformFixture();
  const adapter = createCoreAdapter({ getContext: () => context, platform });
  const controller = createLiveboardController({ adapter, resetLocal: vi.fn(async () => {}), version: '2.0.0' });
  controllers.push(controller);
  await controller.refresh();
  controller.navigate('secrets');
  controller.search('Same');
  controller.scroll(281);
  return {
    controller,
    platform,
    get context() {
      return context;
    },
    replace(next: typeof context) {
      context = next;
    },
    submit: () => (kind === 'recover' ? controller.secrets.primary('pass-a') : controller.secrets.remove('pass-a')),
    async publish(operations: CoreOperation[], secrets = material(kind)) {
      vi.mocked(context.vaultData.secrets).mockImplementation(() => secrets.map((item) => fixturePass(item.secretId)));
      vi.mocked(context.member.liveboard_state_json).mockReturnValue(
        JSON.stringify(coreState({ storageMode: 'shares', secrets, operations })),
      );
      await controller.refresh();
    },
  };
}
function selected(context: ReturnType<typeof preparedContext>, kind: Kind) {
  return vi.mocked(kind === 'recover' ? context.manager.recover_checked : context.manager.delete_secret_checked);
}
function expectExactWrites(context: ReturnType<typeof preparedContext>, kind: Kind, passIds: string[]) {
  const writes = selected(context, kind);
  expect(writes).toHaveBeenCalledTimes(passIds.length);
  // Inspect captured lookup results without calling a wrapper after its finally-free.
  expect(writes.mock.calls.map(([pass]) => vi.mocked(pass.id_str).mock.results[0]?.value)).toEqual(passIds);
  expect(context.manager.recover_js).not.toHaveBeenCalled();
  expect(context.manager.delete_secret).not.toHaveBeenCalled();
  expect(context.manager.continue_local_remove_checked).not.toHaveBeenCalled();
  expect(context.manager.sync_now).not.toHaveBeenCalled();
  expect(context.manager.get_state).not.toHaveBeenCalled();
}

describe('Exact terminal settlement authority through the actual Web composition', () => {
  it.each(kinds)('consumes exact %s Declined once and keeps a distinct explicit retry pending', async (kind) => {
    const s = await setup(kind);
    await s.submit();
    await s.controller.secrets.primary('pass-b');
    const held = s.controller.secrets.state.value.cards['pass-a'];
    expect(held.phase).toBe(kind === 'recover' ? 'waiting' : 'deleting');
    expectExactWrites(s.context, kind, kind === 'recover' ? ['pass-a', 'pass-b'] : ['pass-a']);
    await s.publish([terminal(kind)]);
    expect(s.controller.secrets.state.value.cards['pass-a']).toBe(held);
    expect(held).toMatchObject({ phase: 'masked', value: null, copied: false });
    expect(s.controller.secrets.state.value.cards['pass-b'].phase).toBe('waiting');
    expect(s.controller.secrets.state.value.presentation).toMatchObject({
      active: true,
      query: 'Same',
      scrollTop: 281,
    });
    expectExactWrites(s.context, kind, kind === 'recover' ? ['pass-a', 'pass-b'] : ['pass-a']);

    armRetry(s.context, kind);
    await s.submit();
    await s.publish([terminal(kind)]);
    await s.publish([terminal(kind)]);
    await s.submit();
    await s.controller.secrets.primary('pass-b');
    expect(held.phase).toBe(kind === 'recover' ? 'waiting' : 'deleting');
    expectExactWrites(s.context, kind, kind === 'recover' ? ['pass-a', 'pass-b', 'pass-a'] : ['pass-a', 'pass-a']);
    expect(s.context.manager.recover_checked).toHaveBeenCalledTimes(kind === 'recover' ? 3 : 1);
    if (kind === 'delete') {
      const references = vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls.map(([raw]) => raw);
      expect(
        references.some(
          (raw) => raw.includes(`"requestId":"${secondRequestId}"`) && raw.includes('"curr":18446744073709551613'),
        ),
      ).toBe(true);
    }
    expect(s.platform.copy).not.toHaveBeenCalled();
  });

  const mismatches = ['request', 'subject', 'requester', 'unrelated'] as const;
  for (const kind of kinds) {
    it.each(mismatches)(`does not settle ${kind} from a wrong %s terminal`, async (mismatch) => {
      const s = await setup(kind);
      await s.submit();
      const patch: Partial<CoreOperation> =
        mismatch === 'request'
          ? { requestId: kind === 'recover' ? 'claim-first' : secondRequestId }
          : mismatch === 'subject'
            ? { secretId: 'pass-b' }
            : mismatch === 'requester'
              ? { requesterId: 'peer-a' }
              : { kind: 'removeDevice', requestId: secondRequestId, secretId: undefined, targetDeviceId: 'peer-b' };
      await s.publish([terminal(kind, patch)]);
      await s.submit();
      expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe(kind === 'recover' ? 'waiting' : 'deleting');
      expectExactWrites(s.context, kind, ['pass-a']);
      expect(s.controller.secrets.state.value.presentation).toMatchObject({ query: 'Same', scrollTop: 281 });
    });
  }

  for (const kind of kinds) {
    it.each(['pending', 'accepted'] as const)(`does not grant another ${kind} from canonical %s`, async (status) => {
      const s = await setup(kind);
      await s.submit();
      const current = terminal(kind, {
        clientStatus: status === 'pending' ? 'pending' : kind === 'recover' ? 'accepted' : 'done',
        outcome: status === 'pending' ? undefined : 'accepted',
        responderId: status === 'pending' ? undefined : 'peer-a',
        resolutionSource: status === 'pending' ? 'unknown' : kind === 'recover' ? 'verified' : 'canonical',
        readiness:
          kind === 'recover'
            ? status === 'pending'
              ? 'waiting'
              : 'synchronizing'
            : status === 'pending'
              ? 'waiting'
              : 'settled',
      });
      const secrets = material(kind);
      if (kind === 'recover')
        Object.assign(secrets[0]!, {
          primaryAction: status === 'pending' ? 'waiting' : 'unavailable',
          recoveryClaimId: 'claim-second',
          readiness: status === 'pending' ? 'waiting' : 'synchronizing',
        });
      else if (status === 'pending') Object.assign(secrets[0]!, { canDelete: false, deletionRequestId: requestId });
      await s.publish(
        [current],
        kind === 'delete' && status === 'accepted' ? secrets.filter((item) => item.secretId !== 'pass-a') : secrets,
      );
      await s.submit();
      expectExactWrites(s.context, kind, ['pass-a']);
      await s.controller.secrets.copy('pass-a');
      expect(s.platform.copy).not.toHaveBeenCalled();
      expect(s.context.manager.show_recovered).not.toHaveBeenCalled();
    });
  }

  it('retains Delete Unknown until observation learns the exact UUID from the SAME source', async () => {
    const s = await setup('delete');
    vi.mocked(s.context.manager.reconcile_producer_receipt).mockRejectedValue(new Error('ObservationUnknown'));
    await s.submit();
    await s.publish([terminal('delete')]);
    await s.submit();
    expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('deleting');
    expectExactWrites(s.context, 'delete', ['pass-a']);

    vi.mocked(s.context.manager.reconcile_producer_receipt).mockResolvedValue(deleteReceipt());
    await s.controller.refresh();
    expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('masked');
    for (const [reference] of vi.mocked(s.context.manager.reconcile_producer_receipt).mock.calls) {
      expect(reference).toContain(`"requestId":"${requestId}"`);
      expect(reference).toContain('"curr":18446744073709551615');
      expect(reference).toContain('"prev":18446744073709551614');
    }
    armRetry(s.context, 'delete');
    await s.submit();
    expectExactWrites(s.context, 'delete', ['pass-a', 'pass-a']);
  });

  it.each(kinds)('cannot carry a late %s receipt/observation into the next parent identity', async (kind) => {
    const s = await setup(kind);
    const previous = s.context;
    const late = deferred<string>();
    if (kind === 'recover') vi.mocked(previous.manager.recover_checked).mockReturnValueOnce(late.promise);
    else vi.mocked(previous.manager.reconcile_producer_receipt).mockReturnValueOnce(late.promise);
    const old = s.submit();
    await flushPromises();
    expectExactWrites(previous, kind, ['pass-a']);

    const next = preparedContext(kind);
    next.identity = 'session-b';
    next.epoch = 2;
    armRetry(next, kind);
    s.replace(next);
    await s.controller.refresh();
    await s.submit();
    late.resolve(kind === 'recover' ? checked({ phase: 'unknown' }) : deleteReceipt());
    await old;
    await s.publish([terminal(kind)]);
    await s.submit();
    expect(s.controller.state.value.ready).toBe(true);
    expect(s.controller.secrets.state.value.cards['pass-a']).toMatchObject({
      phase: kind === 'recover' ? 'waiting' : 'deleting',
      value: null,
      copied: false,
    });
    expectExactWrites(next, kind, ['pass-a']);
    expectExactWrites(previous, kind, ['pass-a']);
    expect(s.platform.copy).not.toHaveBeenCalled();
  });

  it.each(kinds)('a tombstoned %s terminal cannot unlock a same-name deterministic Pass retry', async (kind) => {
    const s = await setup(kind);
    await s.submit();
    const removed = s.controller.secrets.state.value.cards['pass-a'];
    await s.publish(
      [terminal(kind)],
      material(kind).filter((item) => item.secretId !== 'pass-a'),
    );
    expect(s.controller.secrets.state.value.cards['pass-a']).toBeUndefined();
    expect(removed.value).toBeNull();
    expect(removed.copied).toBe(false);
    await s.publish([terminal(kind)]);
    const recreated = s.controller.secrets.state.value.cards['pass-a'];
    expect(recreated).not.toBe(removed);
    expect(recreated.secret.name).toBe(removed.secret.name);
    expect(recreated.secret.passId).toBe(removed.secret.passId);
    armRetry(s.context, kind);
    await s.submit();
    await s.publish([terminal(kind)]);
    await s.submit();
    expect(recreated).toMatchObject({ phase: kind === 'recover' ? 'waiting' : 'deleting', value: null, copied: false });
    expectExactWrites(s.context, kind, ['pass-a', 'pass-a']);
  });

  it('settles exact owned Done with current ready Recover once, without automatic Show or retry', async () => {
    const s = await setup('recover');
    await s.submit();
    const held = s.controller.secrets.state.value.cards['pass-a'];
    const done = terminal('recover', { clientStatus: 'done', outcome: 'accepted' });
    await s.publish([done]);
    expect(s.controller.secrets.state.value.cards['pass-a']).toBe(held);
    expect(held).toMatchObject({
      phase: 'masked',
      secret: { primary: { kind: 'recover' } },
      value: null,
      copied: false,
    });
    expectExactWrites(s.context, 'recover', ['pass-a']);
    expect(s.context.manager.show_local_secret).not.toHaveBeenCalled();
    expect(s.context.manager.show_recovered).not.toHaveBeenCalled();
    armRetry(s.context, 'recover');
    await s.submit();
    await s.publish([done]);
    await s.publish([done]);
    await s.submit();
    expect(held.phase).toBe('waiting');
    expectExactWrites(s.context, 'recover', ['pass-a', 'pass-a']);
    expect(s.controller.secrets.state.value.presentation).toMatchObject({ query: 'Same', scrollTop: 281 });
  });

  it.each(['claim', 'pass', 'requester', 'unknown', 'nonready', 'non-recover'] as const)(
    'does not grant Recover from %s completion metadata',
    async (reason) => {
      const s = await setup('recover');
      await s.submit();
      const patch: Partial<CoreOperation> =
        reason === 'claim'
          ? { requestId: 'claim-first' }
          : reason === 'pass'
            ? { secretId: 'pass-b' }
            : reason === 'requester'
              ? { requesterId: 'peer-a' }
              : {};
      const done = terminal('recover', { clientStatus: 'done', outcome: 'accepted', ...patch });
      const secrets = material('recover');
      if (reason === 'nonready') s.context.ready = false;
      if (reason === 'non-recover')
        Object.assign(secrets[0]!, { primaryAction: 'unavailable', readiness: 'synchronizing' });
      await s.publish(reason === 'unknown' ? [] : [done], secrets);
      await s.submit();
      expectExactWrites(s.context, 'recover', ['pass-a']);
      expect(s.controller.secrets.state.value.cards['pass-a'].value).toBeNull();
      expect(s.context.manager.show_recovered).not.toHaveBeenCalled();
      expect(s.context.manager.show_local_secret).not.toHaveBeenCalled();
      if (reason !== 'non-recover') expect(s.controller.secrets.state.value.cards['pass-a'].phase).toBe('waiting');
    },
  );

  it('preserves exact green unread when Done is observed without deliberate visible Show', async () => {
    const s = await setup('recover');
    await s.submit();
    const accepted = terminal('recover', { clientStatus: 'accepted', outcome: 'accepted', readiness: 'ready' });
    const secrets = material('recover');
    Object.assign(secrets[0]!, { primaryAction: 'showRecovered', recoveryClaimId: 'claim-second' });
    await s.publish([accepted], secrets);
    expect(s.controller.state.value.unread['claim-second']).toBe('green');
    await s.publish([terminal('recover', { clientStatus: 'done', outcome: 'accepted' })]);
    expect(s.controller.state.value.unread['claim-second']).toBe('green');
    expectExactWrites(s.context, 'recover', ['pass-a']);
    expect(s.context.manager.show_recovered).not.toHaveBeenCalled();
    expect(s.context.manager.show_local_secret).not.toHaveBeenCalled();
    expect(s.platform.copy).not.toHaveBeenCalled();
  });
});
