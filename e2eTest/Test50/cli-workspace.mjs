import { mkdirSync, mkdtempSync } from 'node:fs';
import { resolve } from 'node:path';

/** One fresh private actor directory per attempt; caller reuses it for all block commands. */
export function createCliActorDirectory(artifactsDirectory, runId, attempt) {
  if (!/^[A-Za-z0-9_-]+$/.test(runId) || !/^[1-9][0-9]*$/.test(String(attempt))) throw new Error('Invalid CLI actor run/attempt identity');
  mkdirSync(artifactsDirectory, { recursive: true });
  return mkdtempSync(resolve(artifactsDirectory, `${runId}-attempt-${attempt}-cli-`));
}
