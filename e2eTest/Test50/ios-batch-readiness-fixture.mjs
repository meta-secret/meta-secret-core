import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { buildPassiveCardFixture } from './ios-scroll-passive-card-fixture.mjs';

function replacePort(text, before, after) {
  assert.equal(text.split(before).length, 2, 'Bind one exact closed fixture port');
  return text.replace(before, after);
}

export function buildBatchReadinessFixture(source, appSource, caseSource) {
  let fixture = buildPassiveCardFixture(source, appSource);
  fixture = fixture.slice(0, fixture.lastIndexOf('MainActor.assumeIsolated {'));
  const start = source.indexOf('    func lbFindSecret(');
  const end = source.indexOf('    @discardableResult\n    func lbFindInList(');
  const find = source.slice(start, end);
  const phase = caseSource.slice(caseSource.indexOf('    private func waitForSecretAfterJoin('), caseSource.indexOf('    private func waitForHidden('));
  assert(find.startsWith('    func lbFindSecret('));
  assert(phase.includes('private func waitForSecretsAfterJoin('));
  fixture = replacePort(fixture, 'let scenario: String', 'var scenario: String');
  fixture = replacePort(fixture, 'var secrets: [String: String] = [:]', 'var secrets: [String: String] = [:]; var secretId: String?');
  fixture = replacePort(fixture, '? "Synthetic" : ""', '? String(id.dropFirst("lb-secret-name-".count)) : ""');
  fixture = replacePort(fixture, 'XCTAssertEqual(identifier, target.identifier, "Exact selector port"); return target',
    'return XCUIElement(identifier, owner: self)');
  fixture = replacePort(fixture, 'XCTAssertEqual(identifier, "lb-secrets-list", "Only current list may be required"); return XCUIElement(identifier)',
    'visits += 1; XCTAssertEqual(identifier, "lb-secrets-list", "Only current list may be required"); return XCUIElement(identifier)');
  return fixture + String.raw`
@MainActor enum ReadinessPort {
    static var waits: [Double] = [], passcodes = 0
}
@MainActor extension XCUIApplication {
    func lbWait(_ predicate: () -> Bool, timeout: TimeInterval = 30, message: String = "") {
        ReadinessPort.waits.append(timeout)
        XCTAssertTrue(predicate(), "Original main/join predicate")
    }
${find}
}
struct SecretDefinition { let name: String }
@MainActor final class CaseFourHarness {
    let app = XCUIApplication("passive-two")
    var phaseSweeps: [Int] = [], phaseDrags: [Int] = [], phaseWaits: [Int] = []
    var mapsValid: [Bool] = [], lastSelectedValid: [Bool] = []
    func enterSimulatorPasscodeIfNeeded() -> Bool { ReadinessPort.passcodes += 1; return false }
${phase}
    func runPhase(_ names: [String]) {
        let sweeps = app.visits, drags = app.drags, waits = ReadinessPort.waits.count
        waitForSecretsAfterJoin(names.map { SecretDefinition(name: $0) }, timeout: 180)
        phaseSweeps.append(app.visits - sweeps); phaseDrags.append(app.drags - drags)
        phaseWaits.append(ReadinessPort.waits.count - waits)
        mapsValid.append(names.allSatisfy { app.lbSelection.secrets[$0] == $0 })
        lastSelectedValid.append(app.lbSelection.secretId == names.last)
    }
    func run() -> [String: Any] {
        app.lbSelection.secrets["unrelated"] = "retained"
        runPhase(["one", "two"])
        app.scenario = "passive-three"
        runPhase(["one", "two", "three"])
        return ["phaseSweeps": phaseSweeps, "phaseDrags": phaseDrags, "phaseWaits": phaseWaits,
                "mapsValid": mapsValid, "lastSelectedValid": lastSelectedValid,
                "unrelatedRetained": app.lbSelection.secrets["unrelated"] == "retained",
                "waits": ReadinessPort.waits, "passcodes": ReadinessPort.passcodes,
                "actions": app.actions, "raw": app.rawGestures, "failures": AssertionPort.failures]
    }
}
MainActor.assumeIsolated {
    let data = try! JSONSerialization.data(withJSONObject: CaseFourHarness().run(), options: [.sortedKeys])
    print("BATCH_READINESS_FIXTURE " + String(decoding: data, as: UTF8.self))
}
`;
}

export function runBatchReadinessFixture(source, appSource, caseSource) {
  const root = mkdtempSync(resolve(tmpdir(), 'ms-r32-batch-readiness-')); mkdirSync(resolve(root, 'cache'));
  try {
    const path = resolve(root, 'fixture.swift'); writeFileSync(path, buildBatchReadinessFixture(source, appSource, caseSource));
    const output = execFileSync('/usr/bin/swift', ['-module-cache-path', resolve(root, 'cache'), path], { encoding: 'utf8', timeout: 25000 });
    const line = output.split('\n').find(value => value.startsWith('BATCH_READINESS_FIXTURE '));
    assert(line, 'Actual CaseFour phases and actual complete search must finish before semantic assertions');
    return JSON.parse(line.slice('BATCH_READINESS_FIXTURE '.length));
  } finally { rmSync(root, { recursive: true, force: true }); }
}
