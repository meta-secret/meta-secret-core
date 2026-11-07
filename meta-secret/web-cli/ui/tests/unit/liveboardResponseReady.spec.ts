import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import RequestLinks from '@/components/liveboard/RequestLinks.vue';
import {
  after,
  back,
  before,
  fresh,
  marker,
  observedBaseline,
  own,
  ownId,
  passive,
  payload,
  pending,
  requestId,
  response,
  settle,
  setup,
  tick,
  vault,
} from './liveboardResponseReadyFixtures';

beforeEach(before);
afterEach(after);
describe('Fresh response publication through actual App/router/VaultView synchronous readiness', () => {
  it('admits the first own waiting row from an evaluated empty collection after exactly one physical sync/read', async () => {
    const s = await setup();
    const previousIdentity = s.session.state.value.identity;
    s.setDTO(payload([]));
    s.auth.masterKey = 'empty-observation-session-key';
    await settle();
    await tick(); // Ordinary auth bootstrap, before the single tested admission read.
    expect(s.session.state.value.identity).not.toBe(previousIdentity);
    expect(s.session.state.value.ready).toBe(true);
    expect(vault(s).request).toBeNull();
    expect(s.controller.state.value.unread).toEqual({});
    await fresh(s, pending());
    expect.soft(vault(s).request).toMatchObject({
      identity: JSON.stringify(['recover', ownId]),
      phase: 'waiting',
      peerIds: ['peer-a', 'peer-b'],
    });
    expect.soft(s.wrapper.findAll('.recovery-packet')).toHaveLength(2);
    passive(s);
    marker(s, 'first-admission-final');
  });

  it.each([true, false])(
    'one fresh Recover response accepted=%s replaces the evaluated pending row before any Show or next event',
    async (accepted) => {
      const s = await setup();
      await observedBaseline(s, pending());
      expect(vault(s).request?.phase).toBe('waiting');
      await fresh(s, response(accepted));
      expect.soft(vault(s).request).toMatchObject({
        identity: JSON.stringify(['recover', ownId]),
        phase: accepted ? 'approved' : 'declined',
        responderId: 'peer-b',
        peerIds: ['peer-b'],
      });
      const packets = s.wrapper.findAll('.recovery-packet');
      expect.soft(packets).toHaveLength(1);
      expect
        .soft(s.wrapper.findAll(accepted ? '.is-approve.recovery-packet' : '.is-decline.recovery-packet'))
        .toHaveLength(1);
      expect.soft(s.wrapper.findAll('.recovery-packet:not(.is-approve):not(.is-decline)')).toHaveLength(0);
      const motions = s.wrapper.findAll('.recovery-packet animateMotion');
      expect.soft(motions.map((motion) => motion.attributes('keyPoints'))).toEqual(['1;0']);
      expect.soft(motions.map((motion) => motion.attributes('repeatCount'))).toEqual(['indefinite']);
      expect(s.controller.state.value.unread[ownId]).toBe(accepted ? 'green' : 'red');
      passive(s);
      marker(s, `fresh-${accepted ? 'green' : 'red'}-final`);
    },
  );

  it('a canonical active Delete decline replaces waiting and draws only the actual responder red', async () => {
    const s = await setup();
    const waiting = own({ kind: 'deleteSecret', requestId });
    await observedBaseline(s, payload([waiting], { canDelete: false, deletionRequestId: requestId }));
    expect(vault(s).request?.phase).toBe('waiting');
    await fresh(
      s,
      payload([
        own({
          ...waiting,
          clientStatus: 'declined',
          outcome: 'declined',
          responderId: 'peer-b',
          resolutionSource: 'canonical',
          eligiblePeerIds: [],
          readiness: 'settled',
        }),
      ]),
    );
    expect.soft(vault(s).request).toMatchObject({
      identity: JSON.stringify(['deleteSecret', requestId]),
      phase: 'declined',
      peerIds: ['peer-b'],
    });
    expect.soft(s.wrapper.findAll('.is-decline.recovery-packet')).toHaveLength(1);
    passive(s);
    marker(s, 'delete-decline-final');
  });

  it('actual accepted Done Delete retires its visual and updates the existing secret list without resurrection', async () => {
    const s = await setup();
    const deletion = own({ kind: 'deleteSecret', requestId });
    await observedBaseline(s, payload([deletion], { canDelete: false, deletionRequestId: requestId }));
    const done = payload([
      own({
        ...deletion,
        clientStatus: 'done',
        outcome: 'accepted',
        responderId: 'peer-b',
        resolutionSource: 'canonical',
        eligiblePeerIds: [],
        readiness: 'settled',
      }),
    ]);
    done.secrets = done.secrets.filter((secret) => secret.secretId !== 'pass-a');
    await fresh(s, done);
    expect.soft(vault(s).request).toBeNull();
    expect.soft(s.wrapper.findAll('.recovery-packet')).toHaveLength(0);
    await s.wrapper.get('a[aria-label="Open Secrets"]').trigger('click');
    await settle();
    expect(s.wrapper.find('[data-secret-id="pass-a"]').exists()).toBe(false);
    expect(s.wrapper.find('[data-secret-id="pass-b"]').exists()).toBe(true);
    await back(s);
    expect(vault(s).request).toBeNull();
    passive(s);
    marker(s, 'delete-done-list-final');
  });

  it.each([
    ['unknown provenance', { resolutionSource: 'unknown' }],
    ['absent responder', { responderId: undefined }],
  ])('never fabricates a reply for %s', async (_name, patch) => {
    const s = await setup();
    await observedBaseline(s, response(true, patch));
    expect(s.wrapper.findAll('.recovery-packet')).toHaveLength(0);
    expect(vault(s).request).toBeNull();
    passive(s);
    marker(s, `unproven-${_name}-final`);
  });

  it('malformed provenance blocks the projection without fabricating a reply', async () => {
    const s = await setup();
    s.setDTO(response(true, { resolutionSource: 'invented' }));
    await s.session.refresh({ requiredSync: true });
    await settle();
    expect(s.session.state.value.ready).toBe(false);
    expect(vault(s).request).toBeNull();
    expect(s.wrapper.findAll('.recovery-packet')).toHaveLength(0);
    passive(s);
  });

  it('an adversarial self-responder metadata row creates no remote arrow', async () => {
    const s = await setup();
    await observedBaseline(s, response(true, { responderId: 'self' }));
    expect.soft(vault(s).request?.peerIds).toEqual([]);
    expect.soft(s.wrapper.findAll('.recovery-packet')).toHaveLength(0);
    passive(s);
    marker(s, 'self-no-remote-endpoint-final');
  });

  it.each([true, false])(
    'already published Recover accepted=%s with empty eligibility draws one proven anchored reverse packet',
    async (accepted) => {
      const s = await setup();
      await observedBaseline(s, response(accepted));
      expect(vault(s).request?.phase).toBe(accepted ? 'approved' : 'declined');
      expect(vault(s).request?.responderId).toBe('peer-b');
      expect.soft(vault(s).request?.peerIds).toEqual(['peer-b']);
      expect.soft(s.wrapper.findAll('.recovery-packet')).toHaveLength(1);
      expect
        .soft(s.wrapper.findAll('.recovery-packet animateMotion').map((motion) => motion.attributes('keyPoints')))
        .toEqual(['1;0']);
      passive(s);
      marker(s, `known-${accepted ? 'green' : 'red'}-endpoint-final`);
    },
  );

  it.each(['requester', 'responder'] as const)(
    'missing %s anchor cannot fabricate a rendered return path',
    (missing) => {
      const wrapper = mount(RequestLinks, {
        props: {
          request: {
            identity: JSON.stringify(['recover', ownId]),
            kind: 'recover',
            phase: 'approved',
            requesterId: 'self',
            peerIds: ['peer-b'],
            responderId: 'peer-b',
            title: '',
            subject: '',
            detail: '',
          },
          anchors: missing === 'requester' ? { 'peer-b': { x: 90, y: 20 } } : { self: { x: 20, y: 20 } },
          bounds: { width: 120, height: 60 },
          reduced: false,
        },
      });
      expect(wrapper.findAll('.recovery-packet')).toHaveLength(0);
      wrapper.unmount();
    },
  );

  it('a foreign requester with the same secret never becomes this device outgoing request', async () => {
    const s = await setup();
    await fresh(s, response(true, { requesterId: 'peer-a' }));
    expect(vault(s).request).toBeNull();
    expect(s.controller.state.value.unread[ownId]).toBeUndefined();
    passive(s);
  });

  it('passive approved rendering preserves unread; only an explicit successful exact Show consumes it', async () => {
    const s = await setup();
    await fresh(s, response());
    expect(s.controller.state.value.unread[ownId]).toBe('green');
    passive(s);
    await s.wrapper.get('a[aria-label="Open Secrets"]').trigger('click');
    await settle();
    const done = response(true, { clientStatus: 'done', readiness: 'done' });
    done.secrets[0] = { ...done.secrets[0]!, primaryAction: 'recover', ...{ recoveryClaimId: undefined } };
    vi.mocked(s.manager.show_recovered).mockImplementation(async () => {
      s.setDTO(done);
      return 'explicit recovered sample';
    });
    await s.wrapper.get('[data-secret-id="pass-a"] [data-secret-action="show"]').trigger('click');
    await settle();
    expect(s.manager.show_recovered).toHaveBeenCalledOnce();
    expect(vi.mocked(s.manager.show_recovered).mock.calls[0]![0].wasm_id_str()).toBe(ownId);
    expect(s.controller.secrets.state.value.cards['pass-a']?.value).not.toBeNull();
    expect(s.controller.state.value.unread[ownId]).toBeUndefined();
    await back(s);
    expect(vault(s).request).toBeNull();
    await fresh(s, response()); // A cached older accepted row cannot revive consumed display.
    expect(vault(s).request).toBeNull();
    expect(s.controller.state.value.unread[ownId]).toBeUndefined();
    marker(s, 'explicit-success-consumed-final');
  });

  it('a failed explicit Show never consumes an approved unread response', async () => {
    const s = await setup();
    await fresh(s, response());
    await s.wrapper.get('a[aria-label="Open Secrets"]').trigger('click');
    await settle();
    vi.mocked(s.manager.show_recovered).mockRejectedValueOnce(new Error('private-unavailable'));
    await s.wrapper.get('[data-secret-id="pass-a"] [data-secret-action="show"]').trigger('click');
    await settle();
    expect(s.controller.state.value.unread[ownId]).toBe('green');
    expect(s.controller.secrets.state.value.cards['pass-a']?.value).toBeNull();
    expect(s.wrapper.find('[data-secret-action="copy"]').exists()).toBe(false);
    marker(s, 'failed-show-unread-final');
  });

  it('a late fresh response cannot publish into a disposed real parent', async () => {
    const s = await setup();
    await fresh(s, pending());
    const read = s.holdRead();
    s.setDTO(response());
    const refreshing = s.session.refresh({ requiredSync: true });
    await settle();
    const old = s.controller;
    s.unmount();
    read.resolve();
    await refreshing;
    await settle();
    expect(old.state.value.vault.request).toBeNull();
    expect(old.state.value.unread[ownId]).toBeUndefined();
    passive(s);
    marker(s, 'late-disposed-final');
  });
});
