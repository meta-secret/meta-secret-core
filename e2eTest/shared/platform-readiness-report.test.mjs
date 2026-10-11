import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import { updateReadinessReport } from './platform-supervisor.mjs';

test('CLI-only participants satisfy readiness without inventing native app requirements', () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'e2e-readiness-report-'));
  const reportPath = resolve(directory, 'readiness.json');
  const previousPath = process.env.E2E_READINESS_PATH;
  writeFileSync(reportPath, JSON.stringify({ platforms: [], status: 'reset-ready' }));
  process.env.E2E_READINESS_PATH = reportPath;

  try {
    updateReadinessReport({
      platform: 'CLI/Core + local HTTP server',
      checks: { cargo: 'cargo 1.x', coreManifest: '/repo/Cargo.toml' },
      requiredNonPlatformParticipants: ['CLI/Core + local HTTP server', 'isolated local HTTP server'],
    });
    const first = JSON.parse(readFileSync(reportPath, 'utf8'));
    assert.equal(first.status, 'reset-ready');
    assert.deepEqual(first.requiredNonPlatformParticipants, ['CLI/Core + local HTTP server', 'isolated local HTTP server']);
    assert.deepEqual(first.nonPlatformParticipants, [
      { participant: 'CLI/Core + local HTTP server', checks: { cargo: 'cargo 1.x', coreManifest: '/repo/Cargo.toml' } },
    ]);

    updateReadinessReport({
      platform: 'isolated local HTTP server',
      checks: { url: 'http://127.0.0.1:3000', healthy: true },
      requiredNonPlatformParticipants: ['CLI/Core + local HTTP server', 'isolated local HTTP server'],
    });
    const updated = JSON.parse(readFileSync(reportPath, 'utf8'));
    assert.equal(updated.status, 'ready');
    assert.equal(updated.nonPlatformParticipants.length, 2);
    assert.equal(updated.nonPlatformParticipants[1].checks.healthy, true);
  } finally {
    if (previousPath === undefined) delete process.env.E2E_READINESS_PATH;
    else process.env.E2E_READINESS_PATH = previousPath;
    rmSync(directory, { recursive: true, force: true });
  }
});
