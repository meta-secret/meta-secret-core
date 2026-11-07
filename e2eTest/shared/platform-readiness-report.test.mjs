import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import { prepareScenarioPlatforms, updateReadinessReport } from './platform-supervisor.mjs';

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

const participants20 = ['CLI/Core + local HTTP server', 'isolated local HTTP server'];
const profile20 = { kind: 'core-http', testNumber: 20, serverUrl: 'http://127.0.0.1:3000',
  stateReset: 'child-owned', requiredNonPlatformParticipants: participants20 };
const cli20 = { platform: participants20[0], checks: {
  cargo: 'cargo 1.fixture', sqlite3: 'sqlite3 3.fixture', coreManifest: '/fixture/Cargo.toml', serverMigration: '/fixture/up.sql',
} };
const server20 = { platform: participants20[1], checks: { url: 'http://127.0.0.1:3000', healthEndpoint: '/hi', healthy: true } };

function withHttpReadiness(run) {
  const directory = mkdtempSync(resolve(tmpdir(), 'e2e-http20-readiness-'));
  const reportPath = resolve(directory, 'readiness.json'); const previous = process.env.E2E_READINESS_PATH;
  const report = { test: 20, platforms: [], status: 'participants-pending', runtimeProfile: profile20,
    requiredNonPlatformParticipants: participants20, nonPlatformParticipants: [] };
  writeFileSync(reportPath, JSON.stringify(report)); process.env.E2E_READINESS_PATH = reportPath;
  const read = () => JSON.parse(readFileSync(reportPath, 'utf8'));
  try { return run({ read, reportPath, directory }); }
  finally {
    if (previous === undefined) delete process.env.E2E_READINESS_PATH;
    else process.env.E2E_READINESS_PATH = previous;
    rmSync(directory, { recursive: true, force: true });
  }
}

function publishInvalid(record) {
  // Rejection or retained pending state is legal. Persisting ready is not.
  try { updateReadinessReport(record); } catch (error) { assert.match(error.message, /proof|participant|readiness|route|check|profile/i); }
}

test('F2 current: unrelated child publication cannot turn an empty Test20 platform report into ready', () => withHttpReadiness(({ read, reportPath }) => {
  writeFileSync(reportPath, JSON.stringify({ test: 20, platforms: [], runtimeProfile: profile20, status: 'participants-pending' }));
  publishInvalid({ platform: 'unrelated participant', checks: { healthy: true } });
  assert.notEqual(read().status, 'ready', 'Zero native platforms is not CLI/Core plus isolated HTTP proof');
}));

test('F2 current: actual CLI and HTTP records require both named proofs and keep the exact child route', () => withHttpReadiness(({ read }) => {
  updateReadinessReport(cli20); assert.notEqual(read().status, 'ready');
  updateReadinessReport(server20); const ready = read();
  assert.equal(ready.status, 'ready');
  assert.deepEqual(ready.requiredNonPlatformParticipants, participants20);
  assert.deepEqual(ready.nonPlatformParticipants.map(({ participant }) => participant), participants20);
  assert.deepEqual(ready.nonPlatformParticipants[1].checks, server20.checks);
}));

test('F2 current: wrong participant and wrong or unhealthy HTTP route never satisfy Test20 readiness', () => {
  for (const record of [
    { ...server20, platform: 'isolated foreign server' },
    { ...server20, checks: { ...server20.checks, url: 'http://localhost:3000' } },
    { ...server20, checks: { ...server20.checks, url: 'http://127.0.0.1:3001' } },
    { ...server20, checks: { ...server20.checks, url: 'https://remote.example:3000' } },
    { ...server20, checks: { ...server20.checks, healthy: false } },
    { ...server20, checks: { url: 'http://127.0.0.1:3000' } },
  ]) withHttpReadiness(({ read }) => {
    updateReadinessReport(cli20); publishInvalid(record);
    assert.notEqual(read().status, 'ready', `Invalid HTTP proof was accepted: ${record.platform}/${record.checks.url ?? 'missing'}`);
  });
});

test('F2 current: missing or failed CLI prerequisites cannot be replaced by an arbitrary checks object', () => {
  for (const checks of [null, {}, { ...cli20.checks, cargo: '' }, { ...cli20.checks, sqlite3: '' },
    { ...cli20.checks, coreManifest: '' }, { ...cli20.checks, serverMigration: '' }, { ...cli20.checks, failed: true }]) {
    withHttpReadiness(({ read }) => {
      updateReadinessReport(server20); publishInvalid({ ...cli20, checks });
      assert.notEqual(read().status, 'ready', 'CLI proof must cover the actual cargo/sqlite/manifest/migration prerequisites');
    });
  }
});

test('F2 current: child cannot erase parent-required participants and a failed later proof revokes ready', () => {
  withHttpReadiness(({ read }) => {
    publishInvalid({ ...cli20, requiredNonPlatformParticipants: [] });
    assert.notEqual(read().status, 'ready');
    assert.deepEqual(read().requiredNonPlatformParticipants, participants20);
  });
  withHttpReadiness(({ read }) => {
    updateReadinessReport(cli20); updateReadinessReport(server20); assert.equal(read().status, 'ready');
    publishInvalid({ ...server20, checks: { ...server20.checks, healthy: false } });
    assert.notEqual(read().status, 'ready', 'An already-ready report cannot keep readiness after failed proof');
  });
});

// Excluded from the first baseline: this injection seam and constructor seeding
// are a Stage4b contract, while the current update function above already runs.
test('F2 deferred: Test20 constructor seeds pending participants before the child can publish anything', async () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'e2e-http20-constructor-'));
  const scenario = JSON.parse(readFileSync(new URL('../Test20/scenario.json', import.meta.url), 'utf8'));
  let nativeStarts = 0;
  try {
    const result = await prepareScenarioPlatforms({ scenario, runtimeProfile: profile20, testNumber: 20,
      attempt: 1, runId: 'source-contract-only', runDirectory: directory,
      verifyEnvironment: () => ({ mode: 'local', selectors: [] }), onRuntimeStarted: () => { nativeStarts++; },
    });
    const report = JSON.parse(readFileSync(result.reportPath, 'utf8'));
    assert.deepEqual(result.platforms, []); assert.equal(nativeStarts, 0);
    assert.notEqual(report.status, 'ready');
    assert.deepEqual(report.requiredNonPlatformParticipants, participants20);
    assert.deepEqual(report.nonPlatformParticipants, []);
    assert.equal(report.runtimeProfile.serverUrl, 'http://127.0.0.1:3000');
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
