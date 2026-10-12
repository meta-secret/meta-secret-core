import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, existsSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createCliActorDirectory } from './cli-workspace.mjs';

test('same run resumes with fresh private actor directories while prior synthetic DB evidence remains', () => {
  const root = mkdtempSync(resolve(tmpdir(), 'cli-workspace-regression-'));
  try {
    const first = createCliActorDirectory(root, 'run-one-cli', 11);
    writeFileSync(resolve(first, 'synthetic-db'), 'old-evidence');
    const resumed = createCliActorDirectory(root, 'run-one-cli', 12);
    const sameAttemptNewActor = createCliActorDirectory(root, 'run-one-cli', 12);
    assert.notEqual(first, resumed); assert.notEqual(resumed, sameAttemptNewActor);
    assert(resumed.includes('attempt-12-cli-'));
    assert.equal(readFileSync(resolve(first, 'synthetic-db'), 'utf8'), 'old-evidence');
    assert.equal(existsSync(resolve(resumed, 'synthetic-db')), false);
    assert.equal(statSync(resumed).mode & 0o777, 0o700);
    writeFileSync(resolve(resumed, 'synthetic-db'), 'same-actor-state');
    // Reconnect commands keep the returned path; no per-command reset/creation.
    assert.equal(readFileSync(resolve(resumed, 'synthetic-db'), 'utf8'), 'same-actor-state');
    const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
    const actor = source.slice(source.indexOf('      createMetaCliRootDevice: async () => {'), source.indexOf('      }, joinWebToCliVault'));
    assert(actor.indexOf('createCliActorDirectory(') < actor.indexOf("['init', 'device'"));
    assert(!actor.includes('rmSync'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});
