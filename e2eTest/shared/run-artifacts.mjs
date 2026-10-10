import { createWriteStream, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createDiagnosticRedactor } from './diagnostic-redaction.mjs';

function timestamp() {
  return new Date().toISOString();
}

function writeStreamEnd(stream) {
  return new Promise((resolvePromise, reject) => {
    stream.once('error', reject);
    stream.end(resolvePromise);
  });
}

/**
 * Own the immutable, redacted logs for one range run. Each retry gets its own
 * attempt file; an existing file is never truncated or overwritten.
 */
export class RunArtifactCollector {
  constructor({ runDirectory, sensitiveValues = [] }) {
    this.runDirectory = resolve(runDirectory);
    this.testDirectory = resolve(this.runDirectory, 'tests');
    mkdirSync(this.testDirectory, { recursive: true });
    this.manifestPath = resolve(this.runDirectory, 'run.json');
    this.summaryPath = resolve(this.runDirectory, 'run.log');
    this.summary = createWriteStream(this.summaryPath, { flags: 'a' });
    this.redactor = createDiagnosticRedactor(sensitiveValues);
    this.activeTest = null;
  }

  writeEvent(message) {
    this.summary.write(`[${timestamp()}] ${message}\n`);
  }

  startTest(testNumber, attempt) {
    if (this.activeTest) throw new Error('Cannot start a test before the previous test log is closed');
    const fileName = `test-${String(testNumber).padStart(2, '0')}-attempt-${attempt}.log`;
    const logPath = resolve(this.testDirectory, fileName);
    const log = createWriteStream(logPath, { flags: 'wx' });
    const channels = new Map(['stdout', 'stderr'].map((channel) => [channel, {
      redactor: this.redactor.createStream(),
      pending: '',
    }]));
    this.activeTest = { log, logPath, channels, testNumber, attempt };
    this.writeEvent(`TEST #${testNumber} attempt ${attempt} started; output: tests/${fileName}`);
    return logPath;
  }

  writeOutput(channel, chunk) {
    if (!this.activeTest || !this.activeTest.channels.has(channel)) {
      throw new Error(`No active test log for ${channel}`);
    }
    const state = this.activeTest.channels.get(channel);
    const safeText = state.redactor.push(Buffer.isBuffer(chunk) ? chunk.toString('utf8') : String(chunk));
    if (!safeText) return;
    process[channel].write(safeText);
    state.pending += safeText;
    const records = state.pending.split(/(?<=\n)/);
    state.pending = '';
    if (records.length > 0 && !records[records.length - 1].endsWith('\n')) {
      state.pending = records.pop();
    }
    for (const record of records) {
      this.activeTest.log.write(`[${timestamp()}][${channel}] ${record}`);
    }
  }

  async finishTest({ status, exitCode, signal, error } = {}) {
    const active = this.activeTest;
    if (!active) return;
    for (const [channel, state] of active.channels) {
      const flushed = state.redactor.flush();
      if (flushed) {
        process[channel].write(flushed);
        state.pending += flushed;
      }
      if (state.pending) {
        active.log.write(`[${timestamp()}][${channel}] ${state.pending}\n`);
        state.pending = '';
      }
    }
    active.log.write(`\n[${timestamp()}][runner] status=${status} exitCode=${exitCode ?? 'null'} signal=${signal ?? 'none'}${error ? ` error=${this.redactor.redactText(error)}` : ''}\n`);
    await writeStreamEnd(active.log);
    this.activeTest = null;
    this.writeEvent(`TEST #${active.testNumber} attempt ${active.attempt}: ${status}; log: ${active.logPath}`);
  }

  async close() {
    await this.finishTest({ status: 'interrupted', exitCode: null, signal: 'unknown' });
    await writeStreamEnd(this.summary);
  }
}
