import { test } from 'node:test';
import assert from 'node:assert/strict';
import { scenarioProgressLine } from './scenario-progress.mjs';

test('numbered steps, role changes and actual native markers remain visible', () => {
  for (const line of ['1. Building local server', '8a. Reusing build-for-testing', '15c. Restarting offline peer', '📱 Roles changed: Cycle 1', '✅ Test #50 block passed', '🧭 TEST #50: checking readiness']) assert(scenarioProgressLine(line), line);
  for (const line of ['E2E: IOS_DELETION_READY_1_ms96-one-ios-main', '[xcodebuild] E2E: IOS_RECOVERY_SECRET_VISIBLE_1_1', '[adb] 10-11 18:23:55.326 2895 3049 I MetaSecretE2E: E2E: ANDROID_DELETION_READY_2_run-two-main']) assert.equal(scenarioProgressLine(line)?.kind, 'native', line);
});

test('Docker cache identifiers, timestamps and Swift compiler source are not milestones', () => {
  for (const line of ['1f58f9372a44  cached layer', '29c182083afe exporting layers', '7 | override func setUpWithError() throws {', '44 | print("E2E: IOS_JOIN_READY")', '2026-10-11T15:24:42.144Z SwiftCompile normal arm64', '#1 [internal] load metadata', '1) unrelated numbered warning', '1.2. compiler version']) assert.equal(scenarioProgressLine(line), null, line);
});
