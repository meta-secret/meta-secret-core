import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { buildBatchReadinessFixture } from './ios-batch-readiness-fixture.mjs';

function port(text, before, after) {
  assert.equal(text.split(before).length, 2, 'Adapt one closed AX/clock port, never an extracted method');
  return text.replace(before, after);
}

export function buildBatchNegativeFixture(source, appSource, caseSource) {
  assert(source.includes('func lbFindSecrets('), 'Negative bank follows the already qualified old-method causal RED');
  let fixture = buildBatchReadinessFixture(source, appSource, caseSource);
  fixture = fixture.slice(0, fixture.lastIndexOf('MainActor.assumeIsolated {'));
  fixture = port(fixture, 'window: Bool = false) {', 'window: Bool = false, labelOverride: String? = nil) {');
  fixture = port(fixture, 'label = id.hasPrefix("lb-secret-name-") ? String(id.dropFirst("lb-secret-name-".count)) : ""',
    'label = labelOverride ?? (id.hasPrefix("lb-secret-name-") ? String(id.dropFirst("lb-secret-name-".count)) : "")');
  fixture = port(fixture, 'snapshots += 1', 'snapshots += 1; ClockPort.now += ClockPort.snapshotCost');
  fixture = port(fixture, 'func lbSecrets(timeout: TimeInterval = 180) { secretsEntries += 1 }',
    'func lbSecrets(timeout: TimeInterval = 180) { BudgetPort.secrets.append(timeout); secretsEntries += 1 }');
  fixture = port(fixture, 'visits += 1; XCTAssertEqual(identifier, "lb-secrets-list", "Only current list may be required"); return XCUIElement(identifier)',
    'visits += 1; BudgetPort.require.append(timeout); ClockPort.now += ClockPort.requireCost; XCTAssertEqual(identifier, "lb-secrets-list", "Only current list may be required"); return XCUIElement(identifier)');
  fixture = port(fixture, 'XCTAssertTrue(predicate(), "Original main/join predicate")',
    'XCTAssertTrue(predicate(), "Original main/join predicate"); ClockPort.now += ClockPort.mainCost');
  fixture = port(fixture, 'let list = SnapshotNode("lb-secrets-list", listFrame)', String.raw`
        if scenario == "missing-expected" { targets.removeAll { $0.identifier == "lb-secret-name-three" } }
        if scenario == "late-duplicate" && drags > 0 {
            targets += [SnapshotNode("lb-secret-late", cardFrame), SnapshotNode("lb-secret-name-late", nameFrame, labelOverride: "one")]
        }
        if scenario == "late-invalid-card" && snapshots > 1 { targets.removeAll { $0.identifier == "lb-secret-one" } }
        if scenario == "malformed-id" {
            targets.removeAll { $0.identifier == "lb-secret-name-one" }
            targets.append(SnapshotNode("lb-secret-name-", nameFrame, labelOverride: "one"))
        }
        if scenario == "disjoint-pages" {
            let hidden = drags == 0 ? "three" : "one"
            targets.removeAll { $0.identifier == "lb-secret-" + hidden || $0.identifier == "lb-secret-name-" + hidden }
        }
        if scenario == "unproven-end" {
            targets.removeAll { $0.identifier == "lb-secret-name-one" }
            var moving = nameFrame; moving.origin.y = snapshots % 2 == 0 ? -20 : 20
            targets.append(SnapshotNode("lb-secret-name-one", moving))
        }
        let list = SnapshotNode("lb-secrets-list", listFrame)`);
  fixture = port(fixture, 'return XCUIElement(identifier, owner: self)',
    'let element = XCUIElement(identifier, owner: self); if scenario == "main-failure" && ["lb-home", "lb-secrets"].contains(identifier) { element.exists = false }; return element');
  const phase = caseSource.slice(caseSource.indexOf('    private func waitForSecretAfterJoin('), caseSource.indexOf('    private func waitForHidden('));
  const setup = caseSource.slice(caseSource.indexOf('        let preExistingSecrets = secretsBeforeIosJoin('), caseSource.indexOf('        if env("E2E_SETUP_ONLY"'));
  assert(setup.includes('print("E2E: IOS_SECRETS_READY")'));
  return fixture + String.raw`
@MainActor enum EmitPort { static var lines: [String] = [] }
@MainActor func print(_ value: String) { EmitPort.lines.append(value) }
@MainActor enum ClockPort {
    static var now = 0.0, snapshotCost = 0.0, mainCost = 0.0, requireCost = 0.0
    static func reset() { now = 0; snapshotCost = 0; mainCost = 0; requireCost = 0 }
}
struct Date: Comparable {
    let tick: Double
    @MainActor init() { tick = ClockPort.now }
    private init(tick: Double) { self.tick = tick }
    func addingTimeInterval(_ interval: Double) -> Date { Date(tick: tick + interval) }
    @MainActor var timeIntervalSinceNow: Double { tick - ClockPort.now }
    static func < (left: Date, right: Date) -> Bool { left.tick < right.tick }
}
@MainActor enum BudgetPort { static var secrets: [Double] = [], require: [Double] = [] }
@MainActor func resetPorts() {
    ClockPort.reset(); BudgetPort.secrets = []; BudgetPort.require = []
    ReadinessPort.waits = []; ReadinessPort.passcodes = 0; AssertionPort.failures = []; EmitPort.lines = []
}
@MainActor func batchCase(_ scenario: String, names: [String] = ["one", "two", "three"], timeout: Double = 180) -> [String: Any] {
    resetPorts()
    ClockPort.snapshotCost = scenario == "unproven-end" ? 0 : 1
    if scenario == "deadline-snapshot" { ClockPort.snapshotCost = 3 }
    if scenario == "deadline-require" { ClockPort.requireCost = 3 }
    let app = XCUIApplication(scenario)
    app.lbSelection.secrets = ["prior": "retained"]; app.lbSelection.secretId = "prior"
    let resolved = app.lbFindSecrets(names, timeout: timeout)
    return ["resolved": resolved.count, "exactMap": names.allSatisfy { resolved[$0] == $0 },
            "mapUnchanged": app.lbSelection.secrets == ["prior": "retained"] && app.lbSelection.secretId == "prior",
            "priorRetained": app.lbSelection.secrets["prior"] == "retained", "lastSelected": app.lbSelection.secretId == names.last,
            "sweeps": app.visits, "snapshots": app.snapshots, "drags": app.drags, "actions": app.actions,
            "raw": app.rawGestures, "failed": !AssertionPort.failures.isEmpty, "elapsed": ClockPort.now,
            "secretsBudgets": BudgetPort.secrets, "requireBudgets": BudgetPort.require]
}
@MainActor final class SetupHarness {
    let app: XCUIApplication
    let fault: String
    init(_ fault: String) { self.fault = fault; app = XCUIApplication(fault == "main-failure" ? fault : "passive-two") }
    func enterSimulatorPasscodeIfNeeded() -> Bool { ReadinessPort.passcodes += 1; return false }
    func secretsBeforeIosJoin(_ values: [SecretDefinition], plan: Int) -> [SecretDefinition] { Array(values.prefix(2)) }
    func configuredSecretNames(plan: Int, stage: String, platform: String) -> [String] { ["three"] }
    func createSecret(_ secret: SecretDefinition) -> Bool { app.actions += 1; app.scenario = fault == "final-failure" ? "missing-card" : "passive-three"; return true }
${phase}
    func performSetup() {
        let secrets = ["one", "two", "three"].map { SecretDefinition(name: $0) }, secretCreationPlan = 0
${setup}
    }
    func exercise() -> [String: Any] {
        performSetup()
        return ["creates": app.actions, "ready": EmitPort.lines.filter { $0 == "E2E: IOS_SECRETS_READY" }.count,
                "failed": !AssertionPort.failures.isEmpty, "sweeps": app.visits]
    }
}
MainActor.assumeIsolated {
    var results: [String: Any] = [:]
    let invalid = ["late-duplicate", "late-invalid-card", "malformed-id", "wrong-id", "wrong-name-id", "missing-card", "missing-name",
        "duplicate-card", "duplicate-name", "offclip-card", "offclip-name", "nonfinite-card", "nonfinite-name", "empty-card", "empty-name",
        "missing-route", "missing-list", "duplicate-route", "duplicate-list", "missing-window", "foreign-card", "foreign-name",
        "equal-frame-foreign-list", "equal-frame-foreign-route", "nonfinite-root", "nonfinite-window", "nonfinite-route", "nonfinite-list",
        "empty-clip", "node-budget", "snapshot-error", "unproven-end"]
    for scenario in invalid { results[scenario] = batchCase(scenario) }
    results["two"] = batchCase("passive-two", names: ["one", "two"])
    results["three"] = batchCase("passive-three")
    results["disjoint"] = batchCase("disjoint-pages")
    results["missing-expected"] = batchCase("missing-expected", timeout: 25)
    results["empty-input"] = batchCase("passive-three", names: [])
    results["duplicate-input"] = batchCase("passive-three", names: ["one", "one"])
    results["empty-input-name"] = batchCase("passive-three", names: [""])
    results["zero-budget"] = batchCase("passive-three", timeout: 0)
    results["large-budget"] = batchCase("passive-three", timeout: 181)
    results["nan-budget"] = batchCase("passive-three", timeout: .nan)
    results["deadline-snapshot"] = batchCase("deadline-snapshot", timeout: 2)
    results["deadline-require"] = batchCase("deadline-require", timeout: 2)
    resetPorts()
    let fresh = XCUIApplication("passive-three")
    let first = fresh.lbFindSecrets(["one", "two", "three"], timeout: 180)
    let firstMap = fresh.lbSelection.secrets, firstSelected = fresh.lbSelection.secretId
    fresh.scenario = "missing-card"
    let second = fresh.lbFindSecrets(["one", "two", "three"], timeout: 180)
    results["fresh-phase"] = ["first": first.count, "second": second.count,
        "unchanged": fresh.lbSelection.secrets == firstMap && fresh.lbSelection.secretId == firstSelected,
        "failed": !AssertionPort.failures.isEmpty, "sweeps": fresh.visits, "actions": fresh.actions]
    resetPorts(); ClockPort.mainCost = 175; ClockPort.snapshotCost = 1
    let phase = CaseFourHarness(); phase.app.scenario = "passive-three"
    phase.app.lbSelection.secrets = ["prior": "retained"]; phase.app.lbSelection.secretId = "prior"
    phase.runPhase(["one", "two", "three"])
    results["shared-budget"] = ["waits": ReadinessPort.waits, "secretsBudgets": BudgetPort.secrets,
        "requireBudgets": BudgetPort.require, "elapsed": ClockPort.now,
        "unchanged": phase.app.lbSelection.secrets == ["prior": "retained"] && phase.app.lbSelection.secretId == "prior",
        "failed": !AssertionPort.failures.isEmpty, "actions": phase.app.actions]
    resetPorts()
    let empty = CaseFourHarness(); empty.runPhase([])
    results["empty-phase"] = ["waits": ReadinessPort.waits.count, "sweeps": empty.app.visits,
        "snapshots": empty.app.snapshots, "passcodes": ReadinessPort.passcodes, "failed": !AssertionPort.failures.isEmpty]
    for fault in ["success", "main-failure", "final-failure"] {
        resetPorts(); results["setup:" + fault] = SetupHarness(fault).exercise()
    }
    let data = try! JSONSerialization.data(withJSONObject: results, options: [.sortedKeys])
    Swift.print("BATCH_NEGATIVE_FIXTURE " + String(decoding: data, as: UTF8.self))
}
`;
}

export function runBatchNegativeFixture(source, appSource, caseSource) {
  const root = mkdtempSync(resolve(tmpdir(), 'ms-r32-batch-negative-')); mkdirSync(resolve(root, 'cache'));
  try {
    const path = resolve(root, 'fixture.swift'); writeFileSync(path, buildBatchNegativeFixture(source, appSource, caseSource));
    const output = execFileSync('/usr/bin/swift', ['-module-cache-path', resolve(root, 'cache'), path], { encoding: 'utf8', timeout: 25000 });
    const line = output.split('\n').find(value => value.startsWith('BATCH_NEGATIVE_FIXTURE '));
    assert(line, 'Whole actual helper/traversal/phase fixture must finish');
    return JSON.parse(line.slice('BATCH_NEGATIVE_FIXTURE '.length));
  } finally { rmSync(root, { recursive: true, force: true }); }
}
