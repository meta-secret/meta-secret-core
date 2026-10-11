import { AsyncLocalStorage } from 'node:async_hooks';

// Error.stack omits AggregateError.errors on Node. Include every nested error
// and cause explicitly, redacting the entire rendering before it is emitted.
export function formatHarnessShutdownError(error, redact) {
  const seen = new Set();
  function render(current, label) {
    if (current && typeof current === 'object') {
      if (seen.has(current)) return `${label}: [already reported]`;
      seen.add(current);
    }
    const lines = [`${label}: ${current?.stack ?? current?.message ?? String(current)}`];
    if (current instanceof AggregateError) {
      Array.from(current.errors).forEach((nested, index) => lines.push(render(nested, `${label}.errors[${index}]`)));
    }
    if (current?.cause !== undefined) lines.push(render(current.cause, `${label}.cause`));
    return lines.join('\n');
  }
  return redact(render(error, 'shutdown'));
}

// Signal handlers and scenario failures share one evidence-before-cleanup
// barrier. A late scenario continuation cannot add a child after the cleanup
// process-tree snapshot; only the barrier's diagnostic/cleanup work may spawn.
export function createHarnessShutdown({ cancelActors, captureDiagnostics, cleanup }) {
  const workScope = new AsyncLocalStorage();
  let shutdownPromise;
  let shuttingDown = false;
  let phase;

  function assertCanSpawn() {
    if (shuttingDown && (!phase || workScope.getStore() !== phase)) {
      throw new Error('Scenario cancelled: shutdown is already in progress');
    }
  }

  function finish({ error, diagnostics = false } = {}) {
    if (shutdownPromise) return shutdownPromise;
    shuttingDown = true;
    // Assign the shared promise before callbacks can trigger a second failure.
    shutdownPromise = Promise.resolve().then(async () => {
      const failures = [];
      try { await cancelActors(error); } catch (failure) { failures.push(failure); }
      if (diagnostics) {
        phase = 'diagnostics';
        try {
          await workScope.run('diagnostics', () => captureDiagnostics(error));
        } catch (failure) { failures.push(failure); }
      }
      phase = 'cleanup';
      try {
        await workScope.run('cleanup', cleanup);
      } catch (failure) { failures.push(failure); }
      phase = undefined;
      if (failures.length) throw new AggregateError(failures, 'Harness shutdown failed', { cause: failures[0] });
    });
    return shutdownPromise;
  }

  return { assertCanSpawn, finish };
}
