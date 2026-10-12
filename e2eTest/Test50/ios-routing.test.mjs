import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { buildDeletionPlan } from './plan.mjs';
import { deletionMarkerLedger } from './markers.mjs';

test('iOS actor persists verified Core routing before every XCTest spawn, including setup without ROLE', () => {
  const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const actor = source.slice(source.indexOf('function startIosJoinTest('), source.indexOf('async function prepareAndroidEmulator()'));
  assert(!actor.includes('if (environment.E2E_ROLE || environment.E2E_NETWORK_ROLE)'));
  assert(actor.includes("networkLossConfig ? networkProxyUrl('ios')"));
  assert(actor.includes("persistedCoreServerUrl !== 'http://127.0.0.1:5202'"));
  assert(actor.includes('coreServerUrl: persistedCoreServerUrl'));
  assert(actor.includes("requireCoreRouting: scenario.mode === 'vault-secret-deletion'"));
  assert(actor.indexOf('coreServerUrl: persistedCoreServerUrl') < actor.indexOf("    'xcodebuild',"));
  assert(actor.includes("watched.waitForMarker('E2E: IOS_TEST50_CORE_ROUTING_VERIFIED'"));
});

test('XCTest forwards persisted routing before app launch and all 88 iOS actors have a gate producer/consumer', () => {
  const source = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift', import.meta.url), 'utf8');
  const setup = source.slice(source.indexOf('override func setUpWithError()'), source.indexOf('    func joinWebInitiatedVault()'));
  assert(setup.indexOf('routing.coreServerUrl') < setup.indexOf('ProcessInfo.processInfo.environment'));
  assert(setup.indexOf('routing.requireCoreRouting == true') < setup.indexOf('app.launch()'));
  assert(setup.indexOf('app.launchEnvironment["METASECRET_E2E_SERVER_URL"]') < setup.indexOf('app.launch()'));
  const ledger = deletionMarkerLedger(buildDeletionPlan('routing'));
  assert.equal(ledger.ledger.filter(({ producer }) => producer === 'ios.setUpWithError').length, 88);
  assert.equal(ledger.produced, 621); assert.equal(ledger.consumed, 541); assert.equal(ledger.diagnostics, 80);
});
