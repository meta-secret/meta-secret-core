/** One owned choice waiter per actor; no competing abandoned marker waits. */
export function createMatchedMarkerChoices({ initialStreams = () => ({}), timers = { setTimeout, clearTimeout }, maxLineChars = 128_000 } = {}) {
  const active = new Set(); let actorFailure = null;
  return {
    get activeCount() { return active.size; },
    wait(choices, { timeoutMs = 180_000, signal } = {}) {
      return new Promise((resolve, reject) => {
        if (actorFailure) return reject(actorFailure);
        if (!Array.isArray(choices) || !choices.length || choices.some((x) => typeof x.marker !== 'string' || !x.marker || /[\r\n]/.test(x.marker)) || new Set(choices.map((x) => x.marker)).size !== choices.length) return reject(new Error('Invalid/duplicate matched marker choices'));
        if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) return reject(new Error('Invalid matched marker deadline'));
        const tails = {}; let timer = null, finished = false;
        const abort = () => finish(new Error('Matched marker choice cancelled by abort'));
        const finish = (error, value) => {
          if (finished) return;
          finished = true; active.delete(waiter);
          if (timer !== null) timers.clearTimeout(timer);
          signal?.removeEventListener('abort', abort);
          if (error) reject(error); else resolve(value);
        };
        const consume = (streams) => {
          const matched = new Map();
          for (const [stream, text] of Object.entries(streams)) {
            const candidate = (tails[stream] ?? '') + String(text);
            const lines = candidate.split('\n'); tails[stream] = lines.pop();
            if (tails[stream].length > maxLineChars || lines.some((line) => line.length > maxLineChars)) throw new Error('Matched marker line exceeds bounded limit');
            for (const raw of lines) {
              const line = raw.endsWith('\r') ? raw.slice(0, -1) : raw;
              for (const choice of choices) if (line.endsWith(choice.marker) && (line.length === choice.marker.length || /\s/.test(line[line.length - choice.marker.length - 1]))) matched.set(choice.marker, choice);
            }
          }
          if (matched.size > 1) throw new Error('Ambiguous actual matched marker alternatives');
          if (matched.size === 1) finish(null, [...matched.values()][0].value);
        };
        const waiter = { consume: (stream, text) => { try { consume({ [stream]: text }); } catch (error) { finish(error); } }, fail: (error) => finish(error) };
        if (signal?.aborted) return abort();
        active.add(waiter); signal?.addEventListener('abort', abort, { once: true });
        try { consume(initialStreams()); } catch (error) { finish(error); }
        if (!finished) timer = timers.setTimeout(() => finish(new Error('Timed out waiting for exact matched marker choice')), timeoutMs);
      });
    },
    push(stream, text) { for (const waiter of [...active]) waiter.consume(stream, text); },
    fail(error) { actorFailure ??= error; for (const waiter of [...active]) waiter.fail(actorFailure); },
  };
}
