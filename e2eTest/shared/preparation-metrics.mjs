const phases = new Set(['snapshot', 'native', 'android-build', 'ios-build', 'wasm-build', 'server-build',
  'supporting-capture', 'product-replay', 'preparation-command', 'f1-validation', 'platform-readiness',
  'state-reset', 'business-scenario', 'scenario-postcheck', 'environment-restore', 'runtime-release', 'runtime-cleanup']);
/** Scalar-only observations. -1 means unavailable; outcome 0 succeeds, 1 throws. */
export function createPreparationMetrics({ observe, now = process.hrtime.bigint, write = () => {} }) {
  const records = []; let sequence = 0;
  return {
    async measure(phase, operation) {
      if (!phases.has(phase) || typeof operation !== 'function') throw Error('Fixed preparation metric phase required');
      const record = { sequence: 0, phase, durationMs: 0, freeBeforeBytes: -1, freeAfterBytes: -1,
        swapBeforeBytes: -1, swapAfterBytes: -1, swapTotalBeforeBytes: -1, swapTotalAfterBytes: -1, outcome: 0, measurementErrors: 0 };
      const readTime = () => { try { const value = now(); if (typeof value !== 'bigint') throw Error(); return value; } catch { record.measurementErrors++; return null; } };
      const observation = async position => {
        try {
          const value = await observe();
          for (const [source, destination] of [['freeBytes', `free${position}Bytes`], ['swapUsedBytes', `swap${position}Bytes`], ['swapTotalBytes', `swapTotal${position}Bytes`]]) {
            if (!Number.isSafeInteger(value?.[source]) || value[source] < 0) throw Error();
            record[destination] = value[source];
          }
        } catch { record.measurementErrors++; }
      };
      await observation('Before'); const start = readTime();
      try { return await operation(); }
      catch (error) { record.outcome = 1; throw error; }
      finally {
        const end = readTime();
        if (start !== null && end !== null) {
          const duration = Number(end - start) / 1e6;
          if (Number.isFinite(duration) && duration >= 0) record.durationMs = duration; else record.measurementErrors++;
        }
        await observation('After');
        record.sequence = ++sequence;
        try { await write({ ...record }); } catch { record.measurementErrors++; }
        records.push(Object.freeze({ ...record }));
      }
    },
    snapshot() { return records.map(record => ({ ...record })); },
  };
}
