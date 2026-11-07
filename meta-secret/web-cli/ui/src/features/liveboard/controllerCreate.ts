import { computed, readonly, shallowRef } from 'vue';
import { createCreateController } from './createController';
import type { CreatePort, CreateReceipt, LiveboardSession } from './types';
import { usable } from './lifecycle';

/** Soft view changes preserve one form; hard clears retain the outstanding write latch. */
export function createControllerCreate(input: { port: CreatePort; allowed(): boolean }) {
  let session: LiveboardSession | null = null;
  let pending = false;
  let receipt: CreateReceipt | null = null;
  let revision = 0;
  let disposed = false;
  const inner = shallowRef(make());
  function make() {
    return createCreateController({
      ...input.port,
      async create(draft) {
        const token = ++revision;
        receipt = null;
        pending = true;
        const result = await input.port.create(draft);
        if (token === revision && !disposed) {
          receipt = { ...result };
          pending = result.phase !== 'confirmed' && result.phase !== 'rejected';
        }
        return result;
      },
    });
  }
  function conceal(reason: 'sync' | 'error' | 'route' | 'hard' = 'hard') {
    if (disposed) return;
    if (reason !== 'hard') {
      inner.value.conceal(reason !== 'sync');
      return;
    }
    inner.value.dispose();
    inner.value = make();
    if (session) inner.value.reconcile(session);
  }
  async function refreshReceipt() {
    if (!receipt || !usable(session) || disposed) return;
    const token = revision;
    const observed = receipt;
    const expected = observed.operationId;
    try {
      const result = await input.port.reconcile(expected);
      if (token !== revision || disposed || receipt !== observed || result.operationId !== expected) return;
      receipt = { ...result };
      if (result.phase === 'confirmed' || result.phase === 'rejected') {
        pending = false;
        receipt = null;
      }
      inner.value.applyReceipt(result);
    } catch {
      // Keep the same Unknown write; the original form already reports a fixed error.
    }
  }
  return {
    state: readonly(computed(() => inner.value.state.value)),
    reconcile(next: LiveboardSession) {
      const changed =
        session &&
        (session.identity !== next.identity || session.vaultId !== next.vaultId || session.member !== next.member);
      if (changed) {
        revision++;
        pending = false;
        receipt = null;
        conceal();
      }
      session = { ...next };
      inner.value.reconcile(next);
    },
    open: () => {
      if (!pending && input.allowed()) inner.value.open();
    },
    change: (patch: Parameters<ReturnType<typeof createCreateController>['change']>[0]) => inner.value.change(patch),
    paste: () => inner.value.paste(),
    pasteText: (text: string) => inner.value.pasteText(text),
    submit: () => (input.allowed() ? inner.value.submit() : Promise.resolve()),
    cancel: () => inner.value.cancel(),
    applyReceipt: (result: CreateReceipt) => inner.value.applyReceipt(result),
    refreshReceipt,
    conceal,
    dispose() {
      disposed = true;
      revision++;
      pending = false;
      receipt = null;
      inner.value.dispose();
      session = null;
    },
  };
}
