import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CreateDraft, CreatePort } from '@/features/liveboard/types';
import { checked, contextFixture, deferred, pass, platformFixture } from './liveboardCoreFixtures';
import {
  after,
  before,
  concealed,
  dismiss,
  localDTO,
  noBusiness,
  retainedQueue,
  settle,
  setup,
  tick,
  type Setup,
} from './liveboardManualErrorReadinessFixtures';

beforeEach(() => {
  before();
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: query === '(prefers-reduced-motion: reduce)',
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }));
});
afterEach(after);
const modes = ['plain', 'seed12', 'seed24'] as const;
function draft(mode: (typeof modes)[number]): CreateDraft {
  return {
    mode: mode === 'plain' ? 'secret' : 'seed',
    name: `Authored ${mode}`,
    value: 'inactive or plain draft',
    wordCount: mode === 'seed24' ? 24 : 12,
    words: Array.from({ length: 24 }, (_, i) => `word${i}`),
  };
}
async function open(s: Setup, value: CreateDraft) {
  await s.wrapper.get('[data-testid="create-secret-open"]').trigger('click');
  s.controller.create.change(value);
  await settle();
  expect(s.controller.create.state.value.open).toBe(true);
  expect(s.controller.create.state.value.draft).toEqual(value);
}
function retained(s: Setup, value: CreateDraft, label: string) {
  expect.soft(s.controller.create.state.value.draft, label).toEqual(value);
  expect.soft((s.wrapper.get('#create-secret-name').element as HTMLInputElement).value, label).toBe(value.name);
  if (value.mode === 'secret')
    expect.soft((s.wrapper.get('#create-secret-value').element as HTMLTextAreaElement).value, label).toBe(value.value);
}
async function manual(s: Setup) {
  const sync = s.holdSync(),
    read = s.holdRead();
  const count = vi.mocked(s.manager.sync_now).mock.calls.length;
  const reads = vi.mocked(s.manager.get_state).mock.calls.length;
  await dismiss(s);
  expect(s.manager.sync_now).toHaveBeenCalledTimes(count + 1);
  expect(s.session.state.value.ready).toBe(false);
  sync.resolve();
  await settle();
  expect(s.manager.get_state).toHaveBeenCalledTimes(reads + 1);
  expect(s.session.state.value.ready).toBe(false);
  read.resolve();
  await settle();
  expect(s.session.state.value.ready).toBe(true);
}
async function knownRejectedParent() {
  const { createCoreAdapter } = await import('@/features/liveboard/coreAdapter');
  const { createLiveboardController } = await import('@/features/liveboard/controller');
  const context = contextFixture(),
    platform = platformFixture();
  const native = createCoreAdapter({ getContext: () => context, platform });
  const writes = vi.fn<CreatePort['create']>().mockResolvedValue({ operationId: 'typed-rejection', phase: 'rejected' });
  const controller = createLiveboardController({
    adapter: { ...native, create: { ...native.create, create: writes } },
    resetLocal: vi.fn(async () => {}),
    version: '2.0.0',
  });
  const value = draft('seed24');
  try {
    await controller.refresh();
    controller.create.open();
    controller.create.change(value);
    await controller.create.submit();
    expect(writes).toHaveBeenCalledExactlyOnceWith(value);
    expect(controller.notifications.state.value.error).toBe('Could not create this secret.');
    expect.soft(controller.create.state.value.open, 'typed parent-port rejection closes').toBe(false);
    expect.soft(controller.create.state.value.draft, 'typed parent-port rejection retains full draft').toEqual(value);
    await controller.refresh();
    controller.create.open();
    expect.soft(controller.create.state.value.draft, 'typed parent-port retry input').toEqual(value);
    controller.create.change(value);
    await controller.create.submit();
    expect(writes).toHaveBeenCalledTimes(2);
    expect(writes.mock.calls[1]![0]).toEqual(value);
    expect(context.manager.cluster_distribution_checked).not.toHaveBeenCalled();
    console.info('CREATE_PARENT_STAGE', 'typed-knownRejected-parent-port-final');
  } finally {
    controller.dispose();
  }
}

describe('Actual parent Create input, error and admitted ownership', () => {
  it('keeps ordinary sync open and preserves complete Plain/12/24 drafts through equal error, manual read and reopen', async () => {
    const normal = await setup('/'),
      value = draft('seed24');
    try {
      await open(normal, value);
      const clipboard = deferred<string>();
      normal.clipboard.readText.mockReturnValueOnce(clipboard.promise);
      const paste = normal.controller.create.paste();
      expect(normal.clipboard.readText).toHaveBeenCalledOnce();
      const sync = normal.holdSync(),
        read = normal.holdRead();
      const count = vi.mocked(normal.manager.sync_now).mock.calls.length;
      const reads = vi.mocked(normal.manager.get_state).mock.calls.length;
      const work = normal.session.refresh({ requiredSync: true });
      await settle();
      expect(normal.manager.sync_now).toHaveBeenCalledTimes(count + 1);
      expect(normal.session.state.value.ready).toBe(false);
      expect.soft(normal.controller.create.state.value.open, 'ordinary sync preserves active form').toBe(true);
      expect.soft((normal.wrapper.get('dialog.create-dialog').element as HTMLDialogElement).open).toBe(true);
      retained(normal, value, 'ordinary blocked sync');
      await normal.controller.create.submit();
      noBusiness(normal);
      sync.resolve();
      await settle();
      expect(normal.manager.get_state).toHaveBeenCalledTimes(reads + 1);
      read.resolve();
      await work;
      clipboard.resolve(Array(24).fill('obsolete').join(' '));
      await paste;
      await settle();
      expect(normal.session.state.value.ready).toBe(true);
      expect.soft(normal.controller.create.state.value.open).toBe(true);
      retained(normal, value, 'fresh same-session sync rejects old clipboard without input loss');
      retainedQueue(normal);
      concealed(normal);
      noBusiness(normal);
      normal.stage('ordinary-sync-open-clipboard-final');
    } finally {
      normal.unmount();
      await settle();
    }
    for (const mode of modes) {
      const s = await setup('/'),
        expected = draft(mode);
      try {
        await open(s, expected);
        for (let occurrence = 0; occurrence < 2; occurrence++) {
          s.controller.showError('Create form unavailable');
          await settle();
          expect(s.controller.notifications.state.value.error).toBe('Create form unavailable');
          expect.soft(s.controller.create.state.value.open, `${mode}/${occurrence} error closes`).toBe(false);
          expect.soft((s.wrapper.get('dialog.create-dialog').element as HTMLDialogElement).open).toBe(false);
          retained(s, expected, `${mode}/${occurrence} error keeps all fields`);
          expect(s.session.state.value.ready).toBe(false);
          await manual(s);
          expect.soft(s.controller.create.state.value.open, 'freshness does not automatically reopen').toBe(false);
          await s.wrapper.get('[data-testid="create-secret-open"]').trigger('click');
          await settle();
          expect(s.controller.create.state.value.open).toBe(true);
          retained(s, expected, `${mode}/${occurrence} actual reopen`);
          s.controller.create.change(expected);
          expect(s.controller.create.state.value.draft).toEqual(expected);
          retainedQueue(s);
          concealed(s);
          noBusiness(s);
          expect(s.physical.maximum).toBe(1);
          s.stage(`${mode}-${occurrence}-error-manual-reopen-final`);
        }
      } finally {
        s.unmount();
        await settle();
      }
    }
    await knownRejectedParent();
  }, 30_000);

  it('preserves actual submitted/Unknown or uncertain Create ownership without replay and settles only exact canonical success', async () => {
    for (const phase of ['queued', 'unknown', 'uncertain'] as const) {
      const s = await setup('/'),
        expected = draft(phase === 'unknown' ? 'seed24' : 'plain');
      try {
        await open(s, expected);
        const native = deferred<string>();
        vi.mocked(s.manager.cluster_distribution_checked).mockReturnValueOnce(native.promise);
        await s.wrapper.get('form.create-form').trigger('submit');
        await settle();
        expect(s.manager.cluster_distribution_checked).toHaveBeenCalledOnce();
        const input = vi.mocked(s.manager.cluster_distribution_checked).mock.calls[0]![0];
        expect(input.pass_id.name).toBe(expected.name);
        expect(input.pass).toBe(expected.mode === 'seed' ? expected.words.slice(0, 24).join(' ') : expected.value);
        expect(input.free).not.toHaveBeenCalled();
        if (phase === 'uncertain') native.reject(new Error('private-entered-Create-failure'));
        else native.resolve(checked({ kind: 'create', secretId: 'pass-new', claimId: undefined, phase }));
        await settle();
        expect(input.free).toHaveBeenCalledOnce();
        expect(s.auth.authenticateWithPasskey).not.toHaveBeenCalled();
        if (phase !== 'uncertain') s.controller.showError('Create outcome still pending');
        await settle();
        expect(s.controller.notifications.state.value.error).not.toBeNull();
        expect.soft(s.controller.create.state.value.open).toBe(false);
        retained(s, expected, `${phase} error preserves authored input, not a replacement command`);
        await manual(s);
        s.controller.create.open();
        s.controller.create.change({ name: expected.name, value: 'must not replace original admitted write' });
        await s.controller.create.submit();
        expect(s.manager.cluster_distribution_checked).toHaveBeenCalledOnce();
        expect(s.controller.create.state.value.open).toBe(false);
        retainedQueue(s);
        concealed(s);
        noBusiness(s, ['cluster_distribution_checked']);
        if (phase !== 'uncertain') {
          s.setDTO({
            ...localDTO(),
            secrets: [
              ...localDTO().secrets,
              { secretId: 'pass-new', primaryAction: 'showLocal', canDelete: true, readiness: 'ready' },
            ],
          });
          const sync = s.holdSync(),
            read = s.holdRead();
          const reads = vi.mocked(s.manager.get_state).mock.calls.length;
          const observing = s.session.refresh({ requiredSync: true });
          await settle();
          sync.resolve();
          await settle();
          expect(s.manager.get_state).toHaveBeenCalledTimes(reads + 1);
          const root = s.roots.at(-1)!;
          vi.mocked(root.member.vault_data().secrets).mockReturnValue([
            pass('pass-a'),
            pass('pass-b'),
            pass('pass-new'),
          ]);
          read.resolve();
          await observing;
          await settle();
          expect(s.session.state.value.ready).toBe(true);
          expect
            .soft(s.controller.create.state.value.draft, `${phase} exact canonical terminal clears input`)
            .toMatchObject({ name: '', value: '', words: Array(24).fill('') });
        } else {
          const reads = vi.mocked(s.manager.get_state).mock.calls.length,
            identity = s.session.state.value.identity;
          s.auth.masterKey = 'genuinely-new-Create-owner';
          await settle();
          await tick();
          expect(vi.mocked(s.manager.get_state).mock.calls.length).toBeGreaterThan(reads);
          expect(s.session.state.value.identity).not.toBe(identity);
          expect(s.session.state.value.ready).toBe(true);
        }
        await open(s, draft('plain'));
        await s.wrapper.get('form.create-form').trigger('submit');
        await settle();
        expect(s.manager.cluster_distribution_checked).toHaveBeenCalledTimes(2);
        const fresh = vi.mocked(s.manager.cluster_distribution_checked).mock.calls[1]![0];
        expect(fresh).not.toBe(input);
        expect(fresh.pass).toBe(draft('plain').value);
        expect(fresh.free).toHaveBeenCalledOnce();
        noBusiness(s, ['cluster_distribution_checked']);
        s.stage(`${phase}-real-native-latch-canonical-or-new-owner-positive-final`);
      } finally {
        s.unmount();
        await settle();
      }
    }
  }, 30_000);

  it('hard-clears full input at actual parent boundaries and rejects late clipboard while allowing a fresh current form', async () => {
    for (const boundary of ['background', 'key', 'member', 'reset', 'dispose', 'idleCancel'] as const) {
      const s = await setup('/'),
        expected = draft('seed24');
      let fresh = s;
      try {
        await open(s, expected);
        const old = s.controller,
          clipboard = deferred<string>();
        s.clipboard.readText.mockReturnValueOnce(clipboard.promise);
        const pending = old.create.paste();
        expect(s.clipboard.readText).toHaveBeenCalledOnce();
        if (boundary === 'background') window.dispatchEvent(new Event('pagehide'));
        if (boundary === 'key') s.auth.masterKey = 'hard-boundary-new-key';
        if (boundary === 'member') {
          s.setMember(false);
          s.publish();
        }
        if (boundary === 'reset') {
          old.requestReset();
          await old.confirmReset();
        }
        if (boundary === 'dispose') s.session.dispose();
        if (boundary === 'idleCancel') await s.wrapper.get('[aria-label="Close creation form"]').trigger('click');
        await settle();
        expect(old.create.state.value.open).toBe(false);
        expect(old.create.state.value.draft).toEqual({
          mode: 'secret',
          name: '',
          value: '',
          wordCount: 12,
          words: Array(24).fill(''),
        });
        clipboard.resolve(Array(24).fill('late private clipboard').join(' '));
        await pending;
        await settle();
        expect(old.create.state.value.draft.words).toEqual(Array(24).fill(''));
        expect(JSON.stringify(old.create.state.value)).not.toContain(expected.value);
        noBusiness(s);
        if (boundary === 'reset') expect(s.manager.clean_up_database).toHaveBeenCalledOnce();
        if (boundary === 'dispose') {
          s.unmount();
          await settle();
          fresh = await setup('/');
        } else {
          if (boundary === 'background') window.dispatchEvent(new Event('pageshow'));
          if (boundary === 'member') {
            s.setMember(true);
            s.publish();
          }
          await settle();
          await tick();
          await s.session.refresh({ requiredSync: true });
          await settle();
        }
        expect(fresh.session.state.value.ready).toBe(true);
        if (boundary === 'member') {
          expect(s.store.isMember).toBe(true);
          const { default: Registration } = await import('@/components/registration/OrbitRegistration.vue');
          s.wrapper.getComponent(Registration).vm.$emit('ready');
          await settle();
        }
        await open(fresh, draft('plain'));
        expect(fresh.controller.create.state.value.draft.words).toEqual(draft('plain').words);
        fresh.controller.create.cancel();
        expect(fresh.controller.create.state.value.draft.value).toBe('');
        noBusiness(fresh);
        retainedQueue(fresh);
        concealed(fresh);
        fresh.stage(`${boundary}-hard-clear-late-input-fresh-current-final`);
      } finally {
        fresh.unmount();
        if (fresh !== s) s.unmount();
        await settle();
      }
    }
  }, 30_000);
});
