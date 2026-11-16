import { readonly, ref } from 'vue';
import { createLifetime, sameSession, usable } from './lifecycle';
import type { CreateDraft, CreatePort, CreateReceipt, CreateView, LiveboardSession } from './types';

const emptyDraft = (): CreateDraft => ({
  mode: 'secret',
  name: '',
  value: '',
  wordCount: 12,
  words: Array<string>(24).fill(''),
});

/** Retains one transient form; an unknown durable write can only reconcile its receipt. */
export function createCreateController(port: CreatePort) {
  const state = ref<CreateView>({ open: false, draft: emptyDraft(), phase: 'idle', receipt: null, pasteHint: '' });
  const lifetime = createLifetime();
  let session: LiveboardSession | null = null;
  let checking = false;
  let editing = 0;
  function clear() {
    lifetime.invalidate();
    editing++;
    state.value = { open: false, draft: emptyDraft(), phase: 'idle', receipt: null, pasteHint: '' };
  }
  function reconcile(next: LiveboardSession) {
    if (lifetime.disposed) return;
    if (!sameSession(session, next) || !next.member) clear();
    session = { ...next };
  }
  function open() {
    if (!usable(session) || lifetime.disposed || state.value.open) return;
    editing++;
    state.value.pasteHint = '';
    state.value.open = true;
  }
  function conceal(close: boolean) {
    if (lifetime.disposed) return;
    editing++;
    state.value = { ...state.value, open: close ? false : state.value.open, pasteHint: '' };
  }
  function change(patch: Partial<CreateDraft>) {
    if (!state.value.open || state.value.phase !== 'idle' || lifetime.disposed) return;
    editing++;
    state.value.draft = {
      ...state.value.draft,
      ...patch,
      words: patch.words
        ? [...patch.words].slice(0, 24).concat(Array<string>(24).fill('')).slice(0, 24)
        : state.value.draft.words,
    };
    state.value.pasteHint = '';
  }
  function pasteText(text: string) {
    if (!state.value.open || state.value.phase !== 'idle' || state.value.draft.mode !== 'seed') return;
    const words = text.trim().split(/\s+/u);
    if (words.length !== 12 && words.length !== 24) {
      state.value.pasteHint = 'Paste a 12 or 24 word phrase.';
      return;
    }
    change({ wordCount: words.length, words });
  }
  async function paste() {
    if (!state.value.open || state.value.phase !== 'idle' || state.value.draft.mode !== 'seed' || !usable(session))
      return;
    const token = lifetime.capture(),
      revision = editing;
    try {
      const text = await port.readClipboard();
      if (lifetime.current(token) && editing === revision && usable(session)) pasteText(text);
    } catch {
      if (lifetime.current(token) && editing === revision && usable(session))
        state.value.pasteHint = 'Clipboard unavailable. Paste the phrase manually.';
    }
  }
  function applyReceipt(receipt: CreateReceipt) {
    if (lifetime.disposed || receipt.operationId !== state.value.receipt?.operationId) return;
    if (receipt.phase === 'confirmed') {
      clear();
      return;
    }
    const token = lifetime.capture(),
      revision = editing;
    state.value = {
      ...state.value,
      receipt: receipt.phase === 'rejected' ? null : { ...receipt },
      phase: receipt.phase === 'rejected' ? 'idle' : 'reconciling',
    };
    if (
      receipt.phase === 'rejected' &&
      lifetime.current(token) &&
      editing === revision &&
      state.value.phase === 'idle' &&
      state.value.receipt === null
    )
      port.reportError('Could not create this secret.');
  }
  async function submit() {
    const draft = state.value.draft;
    if (!state.value.open || state.value.phase !== 'idle' || !usable(session) || lifetime.disposed) return;
    if (
      !draft.name.trim() ||
      (draft.mode === 'secret'
        ? !draft.value.trim()
        : draft.words.slice(0, draft.wordCount).some((word) => !word.trim()))
    )
      return;
    const token = lifetime.capture();
    state.value.phase = 'saving';
    try {
      const receipt = await port.create({ ...draft, words: [...draft.words] });
      if (!lifetime.current(token)) return;
      state.value.receipt = { ...receipt };
      applyReceipt(receipt);
    } catch {
      if (!lifetime.current(token)) return;
      const revision = editing;
      state.value.phase = 'reconciling';
      if (lifetime.current(token) && editing === revision && state.value.phase === 'reconciling')
        port.reportError('Creation outcome is not confirmed. Synchronize before retrying.');
    }
  }
  async function refreshReceipt() {
    const receipt = state.value.receipt;
    if (checking || !receipt || state.value.phase !== 'reconciling' || !usable(session)) return;
    checking = true;
    const token = lifetime.capture();
    try {
      const result = await port.reconcile(receipt.operationId);
      if (lifetime.current(token)) applyReceipt(result);
    } catch {
      if (lifetime.current(token)) port.reportError('Creation outcome is not confirmed.');
    } finally {
      checking = false;
    }
  }
  return {
    state: readonly(state),
    reconcile,
    open,
    change,
    paste,
    pasteText,
    submit,
    refreshReceipt,
    applyReceipt,
    conceal,
    cancel: () => {
      if (state.value.phase === 'idle') clear();
    },
    dispose: () => {
      clear();
      lifetime.dispose();
      session = null;
    },
  };
}
