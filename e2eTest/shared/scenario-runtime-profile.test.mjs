import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { createScenarioRuntimeProfile, assertScenarioReadiness } from './scenario-runtime-profile.mjs';

// Future-module specification: intentionally excluded from the current-function
// RED command. Missing this module is never reported as semantic RED evidence.
const scenario20 = JSON.parse(readFileSync(new URL('../Test20/scenario.json', import.meta.url), 'utf8'));
const scenario1 = JSON.parse(readFileSync(new URL('../Test1/scenario.json', import.meta.url), 'utf8'));
const required = ['CLI/Core + local HTTP server', 'isolated local HTTP server'];

test('F2 profile: only registered Test20 with its exact participant shape selects child-owned Core HTTP', () => {
  const before = JSON.stringify(scenario20);
  const profile = createScenarioRuntimeProfile({ testNumber: 20, scenario: scenario20 });
  assert.equal(profile.kind, 'core-http'); assert.equal(profile.testNumber, 20);
  assert.equal(profile.serverUrl, 'http://127.0.0.1:3000'); assert.equal(profile.stateReset, 'child-owned');
  assert.deepEqual(profile.requiredNonPlatformParticipants, required);
  assert.equal(JSON.stringify(scenario20), before);
});

test('F2 profile: normal UI declarations retain the strict UI contract', () => {
  const before = JSON.stringify(scenario1);
  const profile = createScenarioRuntimeProfile({ testNumber: 1, scenario: scenario1 });
  assert.equal(profile.kind, 'ui'); assert.equal(profile.testNumber, 1);
  assert.equal(JSON.stringify(scenario1), before);
});

test('F2 profile: missing Web fields never infer Test20 and conflicting shapes fail closed', () => {
  for (const input of [
    { testNumber: 19, scenario: scenario20 }, { testNumber: '20', scenario: scenario20 },
    { testNumber: 20, scenario: { ...scenario20, web: scenario1.web } },
    { testNumber: 20, scenario: { ...scenario20, android: scenario1.android } },
    { testNumber: 20, scenario: { ...scenario20, server: { ...scenario20.server, url: 'http://127.0.0.1:3000' } } },
    { testNumber: 20, scenario: { ...scenario20, cli: { participant: 'foreign CLI' } } },
    { testNumber: 1, scenario: { ...scenario1, web: undefined } },
    { testNumber: 1, scenario: { ...scenario1, server: { ...scenario1.server, container: undefined } } },
  ]) assert.throws(() => createScenarioRuntimeProfile(input), /profile|scenario|registered|participant|Web|container|Test.?20/i);
});

test('F2 profile: final child result requires real named route proof as well as successful authoritative exit', () => {
  const profile = createScenarioRuntimeProfile({ testNumber: 20, scenario: scenario20 });
  const report = { test: 20, platforms: [], status: 'ready', runtimeProfile: profile,
    requiredNonPlatformParticipants: required,
    nonPlatformParticipants: [
      { participant: required[0], checks: { cargo: 'cargo 1.fixture', sqlite3: 'sqlite3 3.fixture', coreManifest: '/fixture/Cargo.toml', serverMigration: '/fixture/up.sql' } },
      { participant: required[1], checks: { url: profile.serverUrl, healthEndpoint: '/hi', healthy: true } },
    ],
  };
  const outcome = { exitCode: 0, signal: null, error: null };
  assert.doesNotThrow(() => assertScenarioReadiness({ profile, report, outcome }));
  for (const changed of [
    { ...report, nonPlatformParticipants: [] },
    { ...report, nonPlatformParticipants: report.nonPlatformParticipants.slice(0, 1) },
    { ...report, nonPlatformParticipants: [{ ...report.nonPlatformParticipants[0], checks: {} }, report.nonPlatformParticipants[1]] },
    { ...report, nonPlatformParticipants: [report.nonPlatformParticipants[0], { ...report.nonPlatformParticipants[1], checks: { url: 'http://127.0.0.1:3001', healthy: true } }] },
    { ...report, status: 'participants-pending' },
  ]) assert.throws(() => assertScenarioReadiness({ profile, report: changed, outcome }), /readiness|proof|participant|route|ready/i);
  for (const failed of [{ exitCode: 1 }, { exitCode: null, signal: 'SIGTERM' }, { ...outcome, error: new Error('spawn failed') }]) {
    assert.throws(() => assertScenarioReadiness({ profile, report, outcome: failed }), /exit|child|result|readiness/i);
  }
});
