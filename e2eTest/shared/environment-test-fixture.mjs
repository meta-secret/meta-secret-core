// TEST-only persistent destination allocator; all scenario/target bytes stay in
// their existing isolated fixtures. Never used by the real range/SDK runner.
import assert from 'node:assert/strict';
import {lstatSync, mkdirSync, mkdtempSync, realpathSync, rmSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

export function createEnvironmentTestRun() {
  const parent = fileURLToPath(new URL('../.artifacts', import.meta.url));
  mkdirSync(parent, {recursive: true, mode: 0o700});
  const parentInfo = lstatSync(parent);
  assert.equal(realpathSync(parent), parent); assert(parentInfo.isDirectory() && !parentInfo.isSymbolicLink());
  assert.equal(parentInfo.uid, process.getuid()); assert.equal(parentInfo.mode & 0o022, 0);
  const runDirectory = mkdtempSync(resolve(parent, 'environment-test-'));
  const identity = lstatSync(runDirectory);
  assert.equal(realpathSync(runDirectory), runDirectory); assert.equal(identity.mode & 0o7777, 0o700);
  return {runDirectory, cleanup() {
    let current;
    try { current = lstatSync(runDirectory); } catch (error) { if (error.code === 'ENOENT') return; throw error; }
    assert(current.isDirectory() && !current.isSymbolicLink());
    assert.deepEqual([current.dev, current.ino, current.uid, current.mode & 0o7777],
      [identity.dev, identity.ino, process.getuid(), 0o700], 'Refuse cleanup of a replaced fixture root');
    rmSync(runDirectory, {recursive: true, force: false});
  }};
}
