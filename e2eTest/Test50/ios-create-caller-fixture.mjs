import assert from 'node:assert/strict';
import { buildCreateInputFixture } from './ios-create-input-fixture.mjs';

function span(source, start, end) {
  const from = source.indexOf(start), to = source.indexOf(end, from + start.length);
  assert(from >= 0 && to > from, `Actual caller boundary ${start}`);
  return source.slice(from, to);
}
export function buildCreateCallerFixture(create, caseTwo, caseFour) {
  const base = buildCreateInputFixture(create);
  const twoEntries = span(caseTwo, '    func testCreateVaultThenApproveAndroidAndWeb()', '    private func skipOnboardingIfNeeded(');
  const twoForward = span(caseTwo, '    private func typeSecretNameAndValue(', '    private func configuredSecrets(');
  const fourPhase = span(caseFour, '    private func joinAndroidInitiatedVaultAndRunRecovery()', '        for cycle in recoveryPlan {') + '    }\n';
  const fourCreate = span(caseFour, '    private func typeSecretNameAndValue(', '    private func tap(');
  const callers = String.raw`import Foundation
@MainActor struct SecretDefinition { let name: String; let value: String }
@MainActor struct CreationPlan {}
@MainActor struct CoordinatorScenario {
    let vaultName = "fixture-vault", secrets = [SecretDefinition(name: "fixture-alpha", value: "secondary")]
    let recoveryPlan: [Int] = [], secretCreationPlan = CreationPlan()
}
@MainActor extension XCUIApplication {
    // Batch endpoint compatibility only; actual phase semantics are covered by ios-create-phase.test.
    func lbFindSecrets(_ names: [String], timeout: TimeInterval = 180) -> [String: String] {
        events.append("CALLER_BATCH_ENDPOINT")
        return Dictionary(uniqueKeysWithValues: names.enumerated().map { ($0.element, "caller-id-" + String($0.offset)) })
    }
    func orbitEnterVaultName(_ value: String) {}
    func orbitCreateVault() {}
    func orbitRequestJoin() {}
    func lbAssertValue(_ value: String) {}
    func lbAssertNoPrivateContent(_ value: String) {}
    func lbShow(_ name: String, expected: String? = nil) {}
}
@MainActor class HostCaller {
    let app: XCUIApplication
    var markers: [String] = [], waits = 0, approvals = 0
    init(_ app: XCUIApplication) { self.app = app }
    func print(_ value: String) { markers.append(value) }
    func tap(_ id: String, timeout: TimeInterval = 30) { app.lbTap(id) }
    func enterSimulatorPasscodeIfNeeded() {}
    func waitForVisible(identifier: String, timeout: TimeInterval, failureMessage: String? = nil) { waits += 1 }
    func waitForHidden(identifier: String, timeout: TimeInterval) { waits += 1 }
    func approvePendingJoin(marker: String) { approvals += 1; markers.append(marker) }
    func waitForApproval(platform: String, cycle: Int) { approvals += 1 }
    func env(_ key: String, defaultValue: String) -> String {
        if key == "E2E_SECRET_NAME" { return "fixture-alpha" }
        if key == "E2E_SECRET_VALUE" { return "secondary" }
        if key == "E2E_SETUP_ONLY" { return "1" }
        return defaultValue
    }
    func coordinatorVaultName(defaultValue: String) -> String { "fixture-vault" }
    func configuredSecrets() -> [SecretDefinition] { [SecretDefinition(name: "fixture-alpha", value: "secondary")] }
    func loadCoordinatorScenario() -> CoordinatorScenario? { CoordinatorScenario() }
    func secretDefinitions() -> [SecretDefinition] { configuredSecrets() }
    func recoveryCyclePlan() -> [Int] { [] }
    func secretCreationPlan() -> CreationPlan { CreationPlan() }
    func configuredSecretNames(plan: CreationPlan, stage: String, platform: String? = nil) -> [String] {
        stage == "afterIosJoin" && platform == "ios" ? ["fixture-alpha"] : []
    }
    func secretsBeforeIosJoin(_ secrets: [SecretDefinition], plan: CreationPlan) -> [SecretDefinition] { [] }
    func waitForSecretsAfterJoin(_ secrets: [SecretDefinition], timeout: TimeInterval) -> Bool { waits += 1; return true }
}
@MainActor final class CaseTwoCaller: HostCaller {
${twoEntries}
${twoForward}
    func exercise(_ name: String) throws {
        if name == "two-direct" { try testCreateVaultThenApproveAndroidAndWeb() }
        else { try createVaultAndWaitForJoins() }
    }
}
@MainActor final class CaseFourCaller: HostCaller {
${fourPhase}
${fourCreate}
    func exercise() throws { try joinAndroidInitiatedVaultAndRunRecovery() }
}
`;
  const main = String.raw`import Foundation
MainActor.assumeIsolated {
    var rows: [[String: Any]] = []
    for caller in ["two-direct", "two-configured", "four-phase", "shared-fill", "shared-create"] {
        for mode in ["plain", "drop-first-five"] {
            Clock.now = 0; AssertionPort.failures = []
            let app = XCUIApplication(mode, "fixture-alpha")
            var markers: [String] = [], waits = 0
            if caller.hasPrefix("two-") {
                let owner = CaseTwoCaller(app); try! owner.exercise(caller); markers = owner.markers; waits = owner.waits
            } else if caller == "four-phase" {
                let owner = CaseFourCaller(app); try! owner.exercise(); markers = owner.markers; waits = owner.waits
            } else if caller == "shared-fill" { let _: Any = app.lbFillCreate(name: "fixture-alpha", value: "secondary") }
            else { app.lbCreate(name: "fixture-alpha", value: "secondary") }
            rows.append(["caller": caller, "mode": mode, "failures": AssertionPort.failures.count,
                "attempted": app.attempted, "valueInputs": app.otherInputs, "submits": app.submits, "finds": app.finds,
                "ready": markers.contains { $0 == "E2E: IOS_SECRETS_READY" || $0 == "E2E: IOS_INITIATOR_READY" },
                "added": markers.contains { $0.hasPrefix("E2E: IOS_SECRET_ADDED_") }, "waits": waits])
        }
    }
    let data = try! JSONSerialization.data(withJSONObject: rows, options: [.sortedKeys])
    Swift.print(String(data: data, encoding: .utf8)!)
}
`;
  return { ports: base.ports, owning: base.owning, callers, main };
}
