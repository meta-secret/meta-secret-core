import { createHash, randomUUID } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { captureRunnerIdentity, runnerAuditCommandSpecs, validateRunnerAudit } from './batch-replay-identity.mjs';

const sha256 = value => createHash('sha256').update(value).digest('hex');

/** Fresh F1 evidence belongs to this runner, separately from immutable products. */
export async function createRunnerAudit({ paths, runId, runAndCapture, redactText = value => value, identityPorts = {} }) {
  if (typeof runAndCapture !== 'function' || typeof runId !== 'string' || !runId) throw Error('Runner audit execution context required');
  const before = captureRunnerIdentity(paths, identityPorts);
  const directory = resolve(paths.runDirectory, 'runner-audits', randomUUID());
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const commands = [];
  for (const [index, spec] of runnerAuditCommandSpecs(paths).entries()) {
    const result = await runAndCapture(spec.file, spec.args, { cwd: spec.cwd, timeoutMs: 120000, maxCaptureBytes: 16 * 1024 * 1024 });
    if (result?.exitCode !== 0 || result.signal !== null) throw Error(`Runner F1 failed: ${spec.label}`);
    const outputs = {};
    for (const stream of ['stdout', 'stderr']) {
      const path = resolve(directory, `${index}-${stream}.log`);
      const bytes = String(redactText(String(result[stream] ?? '')));
      writeFileSync(path, bytes, { mode: 0o600, flag: 'wx' });
      outputs[stream] = { path, sha256: sha256(bytes) };
    }
    commands.push({ ...spec, naturalExitCode: result.exitCode, signal: result.signal, ...outputs });
  }
  const after = captureRunnerIdentity(paths, identityPorts);
  if (JSON.stringify(before) !== JSON.stringify(after)) throw Error('Runner changed during F1; audit not accepted');
  const path = resolve(directory, 'audit.json');
  const bytes = JSON.stringify({ version: 1, runId, runnerPid: process.pid, before, after, commands }, null, 2);
  writeFileSync(path, bytes, { mode: 0o600, flag: 'wx' });
  const audit = Object.freeze({ path, sha256: sha256(bytes) });
  validateRunnerAudit({ paths, runId, audit }, identityPorts);
  return audit;
}
