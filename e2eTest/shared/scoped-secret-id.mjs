/** Keep only allowlisted, complete opaque-ID marker payloads, independent of output tails. */
export function createScopedSecretIdCollector() {
  const ids = new Map(); const pending = {}; const waiters = new Set(); let closedError;
  const accept = (line) => {
    const normalized = line.replace(/^\d{2}-\d{2}\s+[\d:.]+\s+\d+\s+\d+\s+[A-Z]\s+MetaSecretE2E:\s*/, '');
    const matched = normalized.match(/^E2E:\s+(ANDROID_DELETION_ID_\d+_[A-Za-z0-9_-]{1,512}):([A-Za-z0-9_-]{8,128})\s*$/);
    if (!matched) return;
    const [, key, id] = matched;
    if (ids.has(key) && ids.get(key) !== id) throw new Error('Conflicting opaque IDs for one scoped native marker');
    if (!ids.has(key) && ids.size >= 32) throw new Error('Scoped native identity marker capacity exceeded');
    ids.set(key, id);
    for (const waiter of [...waiters]) if (waiter.key === key) { clearTimeout(waiter.timer); waiters.delete(waiter); waiter.resolve(id); }
  };
  return {
    push(stream, text) {
      const lines = `${pending[stream] ?? ''}${text}`.split(/\r?\n/);
      pending[stream] = lines.pop().slice(-2048);
      for (const line of lines) accept(line);
    },
    wait(key, timeoutMs = 180000) {
      if (!/^ANDROID_DELETION_ID_\d+_[A-Za-z0-9_-]{1,512}$/.test(key)) throw new Error('Invalid scoped native identity marker key');
      if (ids.has(key)) return Promise.resolve(ids.get(key));
      if (closedError) return Promise.reject(closedError);
      return new Promise((resolve, reject) => {
        const waiter = { key, resolve, reject };
        waiter.timer = setTimeout(() => { waiters.delete(waiter); reject(new Error('Exact scoped native identity payload was not observed')); }, timeoutMs);
        waiters.add(waiter);
      });
    },
    close(error) { closedError ??= error; for (const waiter of waiters) { clearTimeout(waiter.timer); waiter.reject(error); } waiters.clear(); },
  };
}
