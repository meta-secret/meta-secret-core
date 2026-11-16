import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { buildDeletionPlan } from './plan.mjs';
import { deletionMarkerLedger } from './markers.mjs';


function assertBefore(source, earlier, later) {
  const first = source.indexOf(earlier), second = source.indexOf(later);
  assert(first >= 0 && second > first, `Required order: ${earlier} before ${later}`);
}

test('iOS actor persists verified Core routing before every XCTest spawn, including setup without ROLE', () => {
  const source = readFileSync(new URL('../Test4/orchestrator.mjs', import.meta.url), 'utf8');
  const actor = source.slice(source.indexOf('function startIosJoinTest('), source.indexOf('async function prepareAndroidEmulator()'));
  assert(!actor.includes('if (environment.E2E_ROLE || environment.E2E_NETWORK_ROLE)'));
  assert(actor.includes("networkLossConfig ? networkProxyUrl('ios')"));
  assert(actor.includes("persistedCoreServerUrl !== 'http://127.0.0.1:5202'"));
  assert(actor.includes('coreServerUrl: persistedCoreServerUrl'));
  assert(actor.includes("requireCoreRouting: scenario.mode === 'vault-secret-deletion'"));
  assertBefore(actor, 'coreServerUrl: persistedCoreServerUrl', "    'xcodebuild',");
  assert(actor.includes("watched.waitForMarker('E2E: IOS_TEST50_CORE_ROUTING_VERIFIED'"));
});

test('XCTest forwards persisted routing before app launch and all 88 iOS actors have a gate producer/consumer', () => {
  const source = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift', import.meta.url), 'utf8');
  const setup = source.slice(source.indexOf('override func setUpWithError()'), source.indexOf('    func joinWebInitiatedVault()'));
  assertBefore(setup, 'routing.coreServerUrl', 'ProcessInfo.processInfo.environment["E2E_CORE_SERVER_URL"]');
  assertBefore(setup, 'routing.requireCoreRouting == true', 'app.launch()');
  assertBefore(setup, 'app.launchEnvironment["METASECRET_E2E_SERVER_URL"]', 'app.launch()');
  const ledger = deletionMarkerLedger(buildDeletionPlan('routing'));
  assert.equal(ledger.ledger.filter(({ producer }) => producer === 'ios.setUpWithError').length, 88);
  assert.equal(ledger.produced, 621); assert.equal(ledger.consumed, 541); assert.equal(ledger.diagnostics, 80);
});


test('Test50 whole-actor launch excludes bypass, and cancellation proves a real foreground system prompt', () => {
  const root = new URL('../../../meta-secret-compose/iosApp/iosAppUITests/', import.meta.url);
  const source = readFileSync(new URL('CaseEightIosBothOfflineUITest.swift', root), 'utf8');
  const setup = source.slice(source.indexOf('override func setUpWithError()'), source.indexOf('    func joinWebInitiatedVault()'));
  assert(setup.includes('if routing.requireCoreRouting != true'));
  assert.equal((setup.match(/METASECRET_UI_TEST_MODE/g) ?? []).length, 1);
  assertBefore(setup, 'if routing.requireCoreRouting != true', 'app.launchEnvironment["METASECRET_UI_TEST_MODE"]');
  assertBefore(setup, 'IOS_TEST50_CORE_ROUTING_VERIFIED', 'app.launch()');
  const authentication = readFileSync(new URL('LiveboardAuthenticationTestActions.swift', root), 'utf8');
  const cancel = authentication.slice(authentication.indexOf('func lbCancelSystemAuthentication()'));
  assert(cancel.includes('XCTAssertNil(launchEnvironment["METASECRET_UI_TEST_MODE"]'));
  assert(cancel.includes('bundleIdentifier: "com.apple.springboard"'));
  function assertSystemCancelGuards(text) {
    const waited = text.indexOf('cancel.exists && cancel.isHittable');
    const foreground = text.indexOf('XCTAssertEqual(system.state, .runningForeground');
    const tapped = text.indexOf('cancel.tap()');
    assert(waited >= 0 && foreground > waited && tapped > foreground,
      'Actual hittable wait and foreground system ownership must both precede the OS Cancel tap');
  }
  assertSystemCancelGuards(cancel);
  assert.throws(() => assertSystemCancelGuards(cancel.replace('XCTAssertEqual(system.state, .runningForeground', '')));
  assert.throws(() => assertSystemCancelGuards(cancel.replace('cancel.exists && cancel.isHittable', '')));
  assert(cancel.includes('!cancel.exists')); assert(!cancel.includes('orbit-back'));
  const actor = source.slice(source.indexOf('func handleSecretDeletionStep()'), source.indexOf('private func test50RectNumbers('));
  function assertWiredCancel(text) {
    const requester = text.indexOf('case "requester", "cancel":');
    const start = text.indexOf('if role == "cancel" {', requester);
    const end = text.indexOf('enterSimulatorPasscodeIfNeeded()', start);
    assert(requester >= 0 && start > requester && end > start, 'Actual cancel role branch is missing');
    const branch = text.slice(start, end);
    const call = branch.indexOf('app.lbCancelSystemAuthentication()');
    const marker = branch.indexOf('marker("CANCELLED")');
    const returned = branch.indexOf('return', marker);
    assert(call >= 0 && marker > call && returned > marker, 'Actual Cancel must call the real OS helper before its completion marker');
    assert(branch.includes('System Cancel closed the Secret list'));
    assert(branch.includes('System Cancel left Delete pending'));
    assert(branch.includes('app.lbAssertNoPrivateContent(value)'));
    assert(!/orbit-back|orbitTap|cancel\.tap|lbConfirmSystemIfPresent|METASECRET_UI_TEST_MODE|auto.?success/i.test(branch));
  }
  assertWiredCancel(actor);
  assert.throws(() => assertWiredCancel(actor.replace('app.lbCancelSystemAuthentication()', 'app.orbitTap("orbit-back")')));
  assert.throws(() => assertWiredCancel(actor.replace('app.lbCancelSystemAuthentication()', '')));

});

test('clean Test50 observer proves actual idle instead of accepting an error or a missing head alone', () => {
  const root = new URL('../../../meta-secret-compose/iosApp/iosAppUITests/', import.meta.url);
  const actor = readFileSync(new URL('CaseEightIosBothOfflineUITest.swift', root), 'utf8');
  assert(actor.includes('XCTAssertTrue(app.lbInboxIdle(), "Expected the actual idle inbox at the clean observer boundary")'));
  const helper = readFileSync(new URL('LiveboardInboxTestActions.swift', root), 'utf8');
  const idle = helper.slice(helper.indexOf('func lbInboxIdle()'), helper.indexOf('func lbDismissInboxErrorIfPresent()'));
  assert(idle.includes('lbVisible(idle)')); assert(idle.includes('!lbInboxError().exists'));
  assert(idle.includes('lbIncomingCount() == 0')); assert(idle.includes('lbReadIncoming() == nil'));
});

test('controlled iOS keeps true Core routing and rejects present invalid config before MODE or launch', () => {
  const source = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift', import.meta.url), 'utf8');
  const setup = source.slice(source.indexOf('override func setUpWithError()'), source.indexOf('func joinWebInitiatedVault()'));
  assertBefore(setup, 'Self.readStepConfig()', 'METASECRET_UI_TEST_MODE');
  assertBefore(setup, 'routing.requireCoreRouting != true', 'app.launch()');
  assert(setup.includes('routing.requireCoreRouting != true || authProfile == "controlled-business"'));
  assert(setup.includes('#if !targetEnvironment(simulator)'));
  assertBefore(setup, 'required == authProfile && routing.requireCoreRouting == true', 'METASECRET_UI_TEST_MODE');
  const reader = source.slice(source.indexOf('nonisolated private static func readStepConfig()'), source.indexOf('private extension XCUIElement'));
  assert(reader.includes('if let supplied = fields["authProfile"]'));
  assertBefore(reader, 'supplied as? String', 'JSONDecoder().decode');
  assert(reader.includes('["real-os", "controlled-business"].contains(profile)'));
  assert(!reader.includes('try?')); assert(reader.includes('cannot use a legacy fallback'));
  assert(reader.includes('ProcessInfo.processInfo.environment["E2E_AUTH_PROFILE"] == nil'));
  const actor = source.slice(source.indexOf('func handleSecretDeletionStep()'), source.indexOf('private func test50RectNumbers('));
  const start = actor.indexOf('if role == "cancel" && config.authProfile == "controlled-business"');
  const end = actor.indexOf('app.lbTap("lb-delete-\\(id)")', start);
  const branch = actor.slice(start, end);
  assert(start > 0 && end > start); assertBefore(branch, 'app.lbAssertNoPrivateContent(value)', 'marker("AUTH_MANUAL_PENDING")');
  assert(branch.includes('return')); assert(!/lbCancelSystemAuthentication|CANCELLED|lbConfirmSystemIfPresent/.test(branch));
  assert(actor.includes('config.requireCoreRouting == true'));
});
// ARTIFACT-ONLY DRAFT: append only after Root source/test grant. Not executed.
import { mkdtempSync as pinTemp, mkdirSync as pinMkdir, writeFileSync as pinWrite, rmSync as pinRemove } from 'node:fs';
import { resolve as pinResolve } from 'node:path';
import { tmpdir as pinTmpdir } from 'node:os';
import { execFileSync as pinExec } from 'node:child_process';
let optionalPinResults;
function actualOptionalPinResults() {
  if (optionalPinResults) return optionalPinResults;
  const text = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/LiveboardAuthenticationTestActions.swift', import.meta.url), 'utf8');
  const start = text.indexOf('    func lbConfirmSystemIfPresent() -> Bool {');
  const end = text.indexOf('\n    func lbCancelSystemAuthentication()', start);
  assert(start >= 0 && end > start, 'Extract the complete actual owning helper');
  const actual = text.slice(start, end);
  const gates = actual.match(/#if targetEnvironment\(simulator\)/g) ?? [];
  assert(gates.length <= 1, 'Only the single platform conditional may be projected by closed host ports');
  const method = actual.replace('#if targetEnvironment(simulator)', '#if PIN_PROBE_HOST_SIMULATOR');
  const fixture = String.raw`import Foundation
@MainActor enum PinPort {
    static var events: [String] = [], failures: [String] = []
    static var fieldPresent = false, digitPresent = false
    static var expectedPin: String?, typedPin: String?, tappedDigits: [String] = []
    static func reset(_ field: Bool, _ digit: Bool, _ pin: String?) {
        events = []; failures = []; fieldPresent = field; digitPresent = digit
        expectedPin = pin; typedPin = nil; tappedDigits = []
    }
}
@MainActor func XCTFail(_ message: String) { PinPort.failures.append(message) }
@MainActor final class ScopedEnvironment { var environment: [String: String] = [:] }
@MainActor enum ProcessInfo { static let processInfo = ScopedEnvironment() }
@MainActor final class PinSelection { var devicePasscode: String? }
@MainActor final class PinElement {
    let field: Bool, digit: String?
    init(field: Bool, digit: String? = nil) { self.field = field; self.digit = digit }
    func waitForExistence(timeout: TimeInterval) -> Bool {
        PinPort.events.append(field ? "field.wait:\(timeout)" : "digit.wait:\(timeout)")
        return field ? PinPort.fieldPresent : PinPort.digitPresent
    }
    func tap() {
        PinPort.events.append(field ? "field.tap" : "digit.tap")
        if let digit { PinPort.tappedDigits.append(digit) }
    }
    func typeText(_ text: String) { PinPort.events.append("field.type"); PinPort.typedPin = text }
}
@MainActor final class PinFields {
    var firstMatch: PinElement { PinPort.events.append("field.first"); return PinElement(field: true) }
}
@MainActor final class PinButtons {
    subscript(_ key: String) -> PinElement {
        PinPort.events.append("digit.lookup"); return PinElement(field: false, digit: key)
    }
}
@MainActor final class XCUIApplication {
    var launchEnvironment: [String: String] = [:]
    let lbSelection = PinSelection()
    init(bundleIdentifier: String? = nil) {
        if let bundleIdentifier {
            precondition(bundleIdentifier == "com.apple.springboard")
            PinPort.events.append("system.create")
        }
    }
    var secureTextFields: PinFields { PinPort.events.append("fields.lookup"); return PinFields() }
    var buttons: PinButtons { PinPort.events.append("buttons.lookup"); return PinButtons() }
}
@MainActor extension XCUIApplication {
${method}
}
@MainActor func runPinCases() -> [String: Any] {
    var result: [String: Any] = [:]
    func run(_ name: String, _ appEnvironment: [String: String] = [:],
             _ scoped: [String: String] = [:], _ selectedPin: String? = nil,
             _ controls: (field: Bool, digit: Bool) = (false, false)) {
        PinPort.reset(controls.field, controls.digit, selectedPin)
        ProcessInfo.processInfo.environment = scoped
        let app = XCUIApplication()
        app.launchEnvironment = appEnvironment
        app.lbSelection.devicePasscode = selectedPin
        let value = app.lbConfirmSystemIfPresent()
        let pinMatches = PinPort.typedPin == selectedPin
        let digitsMatch = PinPort.tappedDigits == (selectedPin?.map(String.init) ?? [])
        result[name] = ["returned": value, "events": PinPort.events, "failures": PinPort.failures,
                        "typedMatches": pinMatches, "digitsMatch": digitsMatch]
    }
    let controlled = ["METASECRET_UI_TEST_MODE": "true"]
    run("controlled-default", controlled)
    run("controlled-profile", controlled, ["E2E_AUTH_PROFILE": "controlled-business"])
    run("mode-missing"); run("mode-false", ["METASECRET_UI_TEST_MODE": "false"])
    run("mode-empty", ["METASECRET_UI_TEST_MODE": ""]); run("mode-case", ["METASECRET_UI_TEST_MODE": "TRUE"])
    run("profile-real-os", controlled, ["E2E_AUTH_PROFILE": "real-os"])
    run("profile-unknown", controlled, ["E2E_AUTH_PROFILE": "unknown"])
    run("profile-empty", controlled, ["E2E_AUTH_PROFILE": ""])
    run("explicit-scoped-pin", controlled, ["E2E_IOS_DEVICE_PASSCODE": "SYNTHETIC_PREREQUISITE"])
    run("selected-pin", controlled, [:], "2468")
    run("selected-pin-empty", controlled, [:], "")
    run("explicit-scoped-pin-empty", controlled, ["E2E_IOS_DEVICE_PASSCODE": ""])
    run("app-profile-does-not-override-scoped-real", ["METASECRET_UI_TEST_MODE": "true", "E2E_AUTH_PROFILE": "controlled-business"], ["E2E_AUTH_PROFILE": "real-os"])
    run("scoped-controlled-not-app-real", ["METASECRET_UI_TEST_MODE": "true", "E2E_AUTH_PROFILE": "real-os"], ["E2E_AUTH_PROFILE": "controlled-business"])
    run("app-only-pin-not-scoped", ["METASECRET_UI_TEST_MODE": "true", "E2E_IOS_DEVICE_PASSCODE": "SYNTHETIC_APP_ONLY"])
    run("app-only-mode-missing", [:], ["METASECRET_UI_TEST_MODE": "true", "E2E_AUTH_PROFILE": "controlled-business"])
    run("field-pin", [:], [:], "2468", (true, false))
    run("digits-pin", [:], [:], "2468", (false, true))
    run("field-missing-credential", [:], [:], nil, (true, false))
    run("digits-missing-credential", [:], [:], nil, (false, true))
    let repeated = XCUIApplication()
    repeated.launchEnvironment = controlled
    var sameApp: [[String: Any]] = []
    let mutations: [([String: String], String?)] = [
        ([:], nil), (["E2E_AUTH_PROFILE": "real-os"], nil),
        (["E2E_IOS_DEVICE_PASSCODE": "SYNTHETIC_REPEAT_PREREQUISITE"], nil), ([:], ""), ([:], nil)]
    for (scoped, selectedPin) in mutations {
        PinPort.reset(false, false, selectedPin)
        ProcessInfo.processInfo.environment = scoped
        repeated.lbSelection.devicePasscode = selectedPin
        let value = repeated.lbConfirmSystemIfPresent()
        sameApp.append(["returned": value, "events": PinPort.events, "failures": PinPort.failures])
    }
    result["same-app-sequence"] = sameApp
    return result
}
MainActor.assumeIsolated {
    let data = try! JSONSerialization.data(withJSONObject: runPinCases(), options: [.sortedKeys])
    print("PIN_PROBE_FIXTURE " + String(decoding: data, as: UTF8.self))
}
`;
  const directory = pinTemp(pinResolve(pinTmpdir(), 'ms-r32-optional-pin-'));
  try {
    pinMkdir(pinResolve(directory, 'cache'));
    const path = pinResolve(directory, 'fixture.swift'); pinWrite(path, fixture);
    const variants = {};
    for (const simulator of [true, false]) {
      const args = ['-module-cache-path', pinResolve(directory, 'cache'), ...(simulator ? ['-DPIN_PROBE_HOST_SIMULATOR'] : []), path];
      const output = pinExec('/usr/bin/swift', args, { encoding: 'utf8', timeout: 25000 });
      for (const canary of ['2468', 'SYNTHETIC_PREREQUISITE', 'SYNTHETIC_APP_ONLY', 'SYNTHETIC_REPEAT_PREREQUISITE']) {
        assert(!output.includes(canary), 'Captured actual-helper fixture output must omit all credential/metadata canaries');
      }
      const line = output.split('\n').find(value => value.startsWith('PIN_PROBE_FIXTURE '));
      assert(line, 'The actual helper must finish before semantic admission assertions');
      variants[simulator ? 'simulator' : 'physical'] = JSON.parse(line.slice('PIN_PROBE_FIXTURE '.length));
    }
    optionalPinResults = variants; return variants;
  } finally { pinRemove(directory, { recursive: true, force: true }); }
}
const originalAbsentPinEvents = ['system.create', 'fields.lookup', 'field.first', 'field.wait:2.0',
  'buttons.lookup', 'digit.lookup', 'digit.wait:1.0'];

test('actual optional PIN helper completes false then avoids OS probes only for admitted simulator metadata', { timeout: 60000 }, () => {
  const results = actualOptionalPinResults().simulator;
  for (const name of ['controlled-default', 'controlled-profile']) {
    assert.equal(results[name].returned, false); assert.deepEqual(results[name].failures, []);
    assert.equal(results[name].events.length, 0, 'Positive controlled simulator must not construct or probe SpringBoard');
  }
});

test('actual optional PIN helper retains original probes for negative mode profile and PIN prerequisites', () => {
  const results = actualOptionalPinResults().simulator;
  for (const name of ['mode-missing', 'mode-false', 'mode-empty', 'mode-case', 'profile-real-os',
    'profile-unknown', 'profile-empty', 'explicit-scoped-pin', 'explicit-scoped-pin-empty', 'selected-pin', 'selected-pin-empty', 'app-only-mode-missing']) {
    assert.equal(results[name].returned, false, name); assert.deepEqual(results[name].failures, [], name);
    assert.deepEqual(results[name].events, originalAbsentPinEvents, name);
  }
});

test('actual optional PIN admission distinguishes app MODE from scoped XCTest profile and PIN metadata', () => {
  const results = actualOptionalPinResults().simulator;
  assert.deepEqual(results['app-profile-does-not-override-scoped-real'].events, originalAbsentPinEvents);
  for (const name of ['scoped-controlled-not-app-real', 'app-only-pin-not-scoped']) {
    assert.equal(results[name].returned, false); assert.deepEqual(results[name].events, []);
  }
  const sequence = results['same-app-sequence'];
  assert.equal(sequence.length, 5);
  sequence.forEach((result, index) => {
    assert.equal(result.returned, false); assert.deepEqual(result.failures, []);
    assert.deepEqual(result.events, [0, 4].includes(index) ? [] : originalAbsentPinEvents);
  });
});

test('actual optional PIN helper preserves secure-field digits and missing-credential original behavior', () => {
  const results = actualOptionalPinResults().simulator;
  assert.equal(results['field-pin'].returned, true); assert.equal(results['field-pin'].typedMatches, true);
  assert.deepEqual(results['field-pin'].events, ['system.create', 'fields.lookup', 'field.first', 'field.wait:2.0', 'field.tap', 'field.type']);
  assert.equal(results['digits-pin'].returned, true); assert.equal(results['digits-pin'].digitsMatch, true);
  assert.equal(results['digits-pin'].events.filter(value => value === 'digit.tap').length, 4);
  assert.deepEqual(results['digits-pin'].failures, []);
  for (const name of ['field-missing-credential', 'digits-missing-credential']) {
    assert.equal(results[name].returned, false); assert.equal(results[name].failures.length, 1);
    assert(results[name].failures[0].includes('declared devicePasscode'));
    assert(!results[name].events.some(value => value.endsWith('.tap') || value === 'field.type'));
  }
});

test('physical branch never skips the original OS probes even with exact positive app mode', () => {
  const results = actualOptionalPinResults().physical;
  for (const name of ['controlled-default', 'controlled-profile', 'scoped-controlled-not-app-real', 'app-only-pin-not-scoped']) {
    assert.equal(results[name].returned, false); assert.deepEqual(results[name].failures, []);
    assert.deepEqual(results[name].events, originalAbsentPinEvents);
  }
});
