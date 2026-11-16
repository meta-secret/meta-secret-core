import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { scopePorts, scopeCases } from './ios-same-request-scope-fixture.mjs';

const swiftRoot = new URL('../../../meta-secret-compose/iosApp/iosAppUITests/', import.meta.url);
function span(source, first, next) {
  const start = source.indexOf(first), end = source.indexOf(next, start + first.length);
  assert(start >= 0 && end > start, `Complete actual-source span: ${first}`);
  return source.slice(start, end);
}
let hostResults;
export function actualSameRequestResults() {
  if (hostResults) return hostResults;
  const actor = readFileSync(new URL('CaseFourIosConcurrentRecoveryUITest.swift', swiftRoot), 'utf8');
  const privacy = readFileSync(new URL('LiveboardPrivacyTestActions.swift', swiftRoot), 'utf8');
  const secret = readFileSync(new URL('LiveboardSecretTestActions.swift', swiftRoot), 'utf8');
  const selectors = readFileSync(new URL('LiveboardTestActions.swift', swiftRoot), 'utf8');
  const headerStart = actor.indexOf('final class CaseFourIosConcurrentRecoveryUITest: XCTestCase {');
  assert(headerStart >= 0);
  const state = span(actor.slice(headerStart), '{\n', '    override func setUpWithError()').slice(2);
  const wrappers = span(actor, '    private func requestRecovery(', '    private func cycleSet(');
  const request = span(secret, '    func lbRequestRecovery(', '    func lbLabel(');
  const show = span(privacy, '    func lbShow(', '    func lbPrivateGeometry(');
  const selection = span(selectors, '@MainActor\nfinal class LiveboardTestSelection', '@MainActor private var liveboardSelections');
  const snapshot = span(selectors, '    func lbSecretsViewportSnapshot()', '    func lbWaitMain(');
  const lifecycle = span(actor, '    override func setUpWithError()', '    func testJoinAndroidInitiatedVaultAndHandleConcurrentRecovery()');
  // Actual bodies are inserted whole; only XCTest, AX and delivery are closed ports.
  const fixture = String.raw`import Foundation
import CoreGraphics
${scopePorts}
final class HostFault {}
@MainActor enum AssertionPort {
    static var failures: [String] = []
    static var primary: HostFault?
    static func reset() { failures = []; primary = nil; ScopeOutput.reset() }
    static func record(_ message: String, _ fault: HostFault? = nil) {
        failures.append(message); if primary == nil { primary = fault ?? HostFault() }
    }
}
@MainActor func XCTFail(_ message: String = "") { AssertionPort.record(message) }
@MainActor func XCTAssertTrue(_ value: Bool, _ message: String = "") { if !value { XCTFail(message) } }
@MainActor func XCTAssertFalse(_ value: Bool, _ message: String = "") { if value { XCTFail(message) } }
@MainActor func XCTAssertEqual<T: Equatable>(_ a: T, _ b: T, _ message: String = "") { if a != b { XCTFail(message) } }
@MainActor func XCTAssertNotNil<T>(_ value: T?, _ message: String = "") { if value == nil { XCTFail(message) } }
@MainActor final class LiveboardIncomingObservationState {}
struct LiveboardIncoming {}
${selection}
@MainActor final class ScopedProcessInfo {
    var environment = ["METASECRET_E2E_IDENTITY_NAMESPACE": "f2-00000000-0000-4000-8000-000000000001"]
}
@MainActor enum ProcessInfo { static let processInfo = ScopedProcessInfo() }
enum HostElementType: Equatable { case any, other, staticText, button, window }
@MainActor class XCUIElement {
    let identifier: String
    var label: String, value: Any?
    var exists = true, isEnabled = true, isHittable = true
    var frame = CGRect(x: 0, y: 0, width: 300, height: 400)
    var children: [XCUIElement] = []
    var elementType: HostElementType = .other
    weak var owner: XCUIApplication?
    init(_ identifier: String, label: String = "", owner: XCUIApplication? = nil) {
        self.identifier = identifier; self.label = label; self.owner = owner
    }
    func descendants(matching type: HostElementType) -> HostQuery {
        let all = children.flatMap { [$0] + $0.descendants(matching: .any).allElementsBoundByIndex }
        return HostQuery(all.filter { type == .any || $0.elementType == type }, owner: owner)
    }
}
@MainActor final class HostQuery {
    let nodes: [XCUIElement]
    weak var owner: XCUIApplication?
    init(_ nodes: [XCUIElement], owner: XCUIApplication?) { self.nodes = nodes; self.owner = owner }
    var allElementsBoundByIndex: [XCUIElement] { nodes.filter(\.exists) }
    var count: Int { allElementsBoundByIndex.count }
    var firstMatch: XCUIElement { allElementsBoundByIndex.first ?? missing("") }
    func element(boundBy index: Int) -> XCUIElement { allElementsBoundByIndex[index] }
    func matching(identifier: String) -> HostQuery { HostQuery(nodes.filter { $0.identifier == identifier }, owner: owner) }
    func matching(_ predicate: NSPredicate) -> HostQuery {
        HostQuery(nodes.filter { predicate.evaluate(with: ["identifier": $0.identifier, "label": $0.label]) }, owner: owner)
    }
    func missing(_ id: String) -> XCUIElement { let node = XCUIElement(id); node.exists = false; node.isEnabled = false; return node }
    subscript(_ identifier: String) -> XCUIElement {
        if identifier.hasPrefix("lb-copy-") { owner?.copyLookups += 1 }
        return matching(identifier: identifier).allElementsBoundByIndex.first ?? missing(identifier)
    }
}
@MainActor final class XCUIApplication: XCUIElement {
    static let passId = "CONTROLLED_OPAQUE_ID"
    static let secretName = "SAME_REQUEST_NAME_CANARY"
    static let secretValue = "SAME_REQUEST_VALUE_CANARY"
    var lbSelection = LiveboardTestSelection()
    var launchEnvironment = ProcessInfo.processInfo.environment
    var lookups = 0, exactReaches = 0, recovers = 0, shows = 0, pinCalls = 0
    var valueRequires = 0, copyLookups = 0, predicateCalls = 0
    var inputsMatched = true, events: [String] = []
    var tapAttempts = 0, recoverAttempts = 0, showAttempts = 0
    var fault = "", reachable = true, tokenAtInput: Bool?
    var scopeMode = "", snapshotCalls = 0
    let injectedFault = HostFault()
    var afterPending: (() -> Void)?, afterReady: (() -> Void)?, onShowInput: (() -> Void)?
    var route: XCUIElement!, list: XCUIElement!, card: XCUIElement!, nameNode: XCUIElement!
    let primary = XCUIElement("lb-primary-" + XCUIApplication.passId, label: "Recover")
    let delete = XCUIElement("lb-delete-" + XCUIApplication.passId)
    let disclosed = XCUIElement("lb-value-" + XCUIApplication.passId, label: XCUIApplication.secretValue)
    let copy = XCUIElement("lb-copy-" + XCUIApplication.passId)
    override init(_ identifier: String = "application", label: String = "", owner: XCUIApplication? = nil) {
        super.init(identifier, label: label, owner: owner)
        self.owner = self
        route = XCUIElement("lb-secrets", owner: self)
        list = XCUIElement("lb-secrets-list", owner: self)
        card = XCUIElement("lb-secret-" + Self.passId, owner: self)
        let name = XCUIElement("lb-secret-name-" + Self.passId, label: Self.secretName, owner: self)
        nameNode = name; name.elementType = .staticText; primary.elementType = .button; delete.elementType = .button
        for node in [primary, delete, disclosed, copy] { node.owner = self }
        disclosed.exists = false; copy.exists = false
        card.children = [name, primary, delete, disclosed, copy]; list.children = [card]
        route.children = [list]; children = [route]
    }
    func lbFindSecret(_ name: String, timeout: TimeInterval = 180) -> String {
        events.append("lookup"); lookups += 1
        XCTAssertEqual(name, Self.secretName, "Closed full-name resolver input changed")
        lbSelection.secrets[name] = Self.passId; lbSelection.secretId = Self.passId
        return Self.passId
    }
    func lbFindInList(_ intent: String) -> Bool {
        events.append("exact-reach"); exactReaches += 1
        if fault == "reach" { AssertionPort.record("OWNED_REACH", injectedFault) }
        guard reachable else { return false }
        if children.contains(where: { $0.identifier == "lb-home" }) { children = [route] } // Closed ordinary route navigation only.
        return descendants(matching: .any).matching(identifier: intent).count == 1
    }
    func lbRequire(_ id: String, timeout: TimeInterval = 30,
                   observedPredicate: ((Int, Bool?) -> Void)? = nil) -> XCUIElement {
        let query = descendants(matching: .any).matching(identifier: id)
        if fault == "value" && id.hasPrefix("lb-value-") { AssertionPort.record("OWNED_VALUE", injectedFault) }
        let node = query.firstMatch
        observedPredicate?(query.count, node.exists)
        XCTAssertEqual(query.count, 1, "Closed actual owner target unavailable")
        if id.hasPrefix("lb-value-") { valueRequires += 1; events.append("require-value") }
        else if id.hasPrefix("lb-primary-") { events.append("require-primary") }
        return node
    }
    func lbVisible(_ element: XCUIElement) -> Bool { element.exists }
    func lbLabel(_ element: XCUIElement, matches choices: [String]) -> Bool { choices.contains(element.label) }
    func lbWait(_ predicate: @escaping () -> Bool, timeout: TimeInterval = 30,
                message: String = "Closed actual owner predicate failed") {
        predicateCalls += 1; events.append("wait")
        let passed = predicate()
        if !passed && fault == "pending" { AssertionPort.record("OWNED_PENDING", injectedFault) }
        if !passed && fault == "ready" { AssertionPort.record("OWNED_READY", injectedFault) }
        XCTAssertTrue(passed, message)
        if recovers > 0 && shows == 0 && primary.label == "Recover" && !primary.isEnabled {
            let hook = afterPending; afterPending = nil; hook?()
        }
        if primary.label == "Show" { let hook = afterReady; afterReady = nil; hook?() }
    }
    func lbTap(_ id: String, timeout: TimeInterval = 30, beforeInput: (() -> Bool)? = nil) {
        tapAttempts += 1
        if primary.label == "Recover" { recoverAttempts += 1 }
        if primary.label == "Show" { showAttempts += 1 }
        inputsMatched = inputsMatched && id == primary.identifier
        guard AssertionPort.failures.isEmpty else { XCTFail("Input after failed owner gate"); return }
        if let beforeInput, !beforeInput() {
            XCTFail("Fresh action scope changed at the actual input boundary"); return
        }
        if primary.label == "Recover" {
            recovers += 1; events.append("recover-input"); primary.isEnabled = fault == "pending"; delete.isEnabled = false
        } else if primary.label == "Show" {
            shows += 1; events.append("show-input"); primary.label = "Hide"
            disclosed.exists = fault != "value"; copy.exists = true
            let hook = onShowInput; onShowInput = nil; hook?()
        } else { XCTFail("Unexpected input instead of actual Recover or Show") }
    }
    func approveOriginalRequest() { primary.label = "Show"; primary.isEnabled = true; delete.isEnabled = true }
    func lbConfirmSystemIfPresent() -> Bool {
        pinCalls += 1; events.append("pin")
        if fault == "auth" { AssertionPort.record("OWNED_AUTH", injectedFault) }
        return false
    }
    func lbCloseValueOrPending() {
        events.append("close")
        if disclosed.exists { disclosed.exists = false; copy.exists = false; primary.label = "Recover" }
        else { children = [XCUIElement("lb-home")] }
    }
    func lbApplyTestIdentity() { launchEnvironment = ProcessInfo.processInfo.environment }
    func lbResetSelectors() { lbSelection = LiveboardTestSelection() }
    func launch() {}
    func duplicate(_ element: XCUIElement, in parent: XCUIElement) {
        let node = XCUIElement(element.identifier, label: element.label, owner: self)
        node.isEnabled = element.isEnabled; node.elementType = element.elementType; parent.children.append(node)
    }
${request}
${show}
${snapshot}
}
@MainActor class HostXCTestCase {
    var continueAfterFailure = true
    func setUpWithError() throws {}
    func tearDownWithError() throws {}
}
@MainActor final class ActualCaseFour: HostXCTestCase {
${state}
    init(_ app: XCUIApplication) { self.app = app; super.init() }
${lifecycle}
${wrappers}
    func driveRequest(_ name: String, _ cycle: Int) { requestRecovery(name, cycle: cycle) }
    func driveShow(_ name: String, _ cycle: Int) { showAcceptedSecret(name, cycle: cycle) }
    func driveClose() { closeShowSecretDialog() }
    func replaceApp(_ value: XCUIApplication) { app = value }
    func hasToken() -> Bool { pendingRecoveryShow != nil }
    func currentApp() -> XCUIApplication { app }
}
@MainActor func runCases() -> [String: Any] {
    var output: [String: Any] = [:]
    for mode in ["same-request", "default-show", "expected-show"] {
        AssertionPort.reset()
        let app = XCUIApplication()
        if mode == "same-request" {
            let owner = ActualCaseFour(app)
            owner.driveRequest(XCUIApplication.secretName, 1)
            app.approveOriginalRequest()
            owner.driveShow(XCUIApplication.secretName, 1)
        } else {
            app.approveOriginalRequest()
            app.lbShow(XCUIApplication.secretName, expected: mode == "expected-show" ? XCUIApplication.secretValue : nil)
        }
        output[mode] = ["failures": AssertionPort.failures, "lookups": app.lookups,
            "recovers": app.recovers, "shows": app.shows, "pins": app.pinCalls,
            "valueRequires": app.valueRequires, "valueExists": app.disclosed.exists,
            "copyLookups": app.copyLookups, "predicates": app.predicateCalls,
            "inputsMatched": app.inputsMatched, "events": app.events]
    }
    return output
}
@MainActor func matrixCases() -> [String: Any] {
    var output: [String: Any] = [:]
    let key = "METASECRET_E2E_IDENTITY_NAMESPACE"
    let namespace = "f2-00000000-0000-4000-8000-000000000001"
    let cases = ["missing", "cycle", "name", "app", "selector-reset", "app-namespace", "scoped-namespace",
        "missing-namespace", "invalid-namespace", "lost-namespace", "both-namespace", "selected-id", "map-id",
        "pending", "pending-delete", "pending-primary", "pending-duplicate", "pending-empty-id", "pending-map",
        "new-request-failed", "missing-card", "missing-route", "missing-list", "missing-name", "renamed", "duplicate-card", "duplicate-name",
        "duplicate-primary", "prefix-collision", "detached-name", "detached-primary", "detached-card", "detached-list", "duplicate-route",
        "duplicate-list", "wrong-primary-type", "reach", "ready", "wrong-label", "after-ready-name", "after-ready-id",
        "after-ready-primary", "after-ready-disabled", "after-ready-label", "unrelated-same-name", "home-incoming", "repeated", "reentrant", "next-cycle",
        "setup", "teardown", "revealed-close", "auth", "value"]
    for mode in cases {
        AssertionPort.reset(); ProcessInfo.processInfo.environment = [key: namespace]
        let app = XCUIApplication(), owner = ActualCaseFour(XCUIApplication())
        owner.replaceApp(app)
        var tracked = [app]
        if mode == "pending" || mode == "new-request-failed" { app.fault = mode == "pending" ? "pending" : "" }
        if mode == "missing-namespace" { app.launchEnvironment.removeValue(forKey: key) }
        if mode == "invalid-namespace" { app.launchEnvironment[key] = "invalid"; ProcessInfo.processInfo.environment[key] = "invalid" }
        if mode == "pending-delete" { app.afterPending = { app.delete.exists = false } }
        if mode == "pending-primary" { app.afterPending = { app.primary.exists = false } }
        if mode == "pending-empty-id" { app.afterPending = { app.lbSelection.secretId = "" } }
        if mode == "pending-map" { app.afterPending = { app.lbSelection.secrets[XCUIApplication.secretName] = "OTHER_ID" } }
        if mode == "pending-duplicate" { app.afterPending = { app.duplicate(app.primary, in: app.card) } }
        if mode != "missing" { owner.driveRequest(XCUIApplication.secretName, 1) }
        else { app.lbSelection.secretId = XCUIApplication.passId; app.lbSelection.secrets[XCUIApplication.secretName] = XCUIApplication.passId }
        let issued = owner.hasToken()
        if mode == "new-request-failed" {
            app.primary.label = "Recover"; app.primary.isEnabled = true; app.fault = "pending"
            owner.driveRequest(XCUIApplication.secretName, 2)
        }
        if mode == "app" { let other = XCUIApplication(); other.lbSelection = app.lbSelection; tracked.append(other); owner.replaceApp(other) }
        if mode == "selector-reset" {
            app.lbResetSelectors(); app.lbSelection.secretId = XCUIApplication.passId
            app.lbSelection.secrets[XCUIApplication.secretName] = XCUIApplication.passId
        }
        if mode == "app-namespace" { app.launchEnvironment[key] = "f2-00000000-0000-4000-8000-000000000002" }
        if mode == "scoped-namespace" { ProcessInfo.processInfo.environment[key] = "f2-00000000-0000-4000-8000-000000000002" }
        if mode == "lost-namespace" { app.launchEnvironment.removeValue(forKey: key) }
        if mode == "both-namespace" {
            app.launchEnvironment[key] = "f2-00000000-0000-4000-8000-000000000002"
            ProcessInfo.processInfo.environment = app.launchEnvironment
        }
        if mode == "selected-id" { app.lbSelection.secretId = "OTHER_ID" }
        if mode == "map-id" { app.lbSelection.secrets[XCUIApplication.secretName] = "OTHER_ID" }
        if mode == "home-incoming" { owner.driveClose(); app.lbSelection.incoming = LiveboardIncoming() }
        if mode == "missing-card" || mode == "prefix-collision" { app.card.exists = false }
        if mode == "prefix-collision" { app.list.children.append(XCUIElement("lb-secret-" + XCUIApplication.passId + "-suffix")) }
        if mode == "missing-route" { app.route.exists = false }
        if mode == "missing-list" { app.list.exists = false }
        if mode == "missing-name" { app.nameNode.exists = false }
        if mode == "renamed" { app.nameNode.label = "RENAMED" }
        if mode == "duplicate-card" { app.duplicate(app.card, in: app.list) }
        if mode == "duplicate-name" { app.duplicate(app.nameNode, in: app.card) }
        if mode == "duplicate-primary" { app.duplicate(app.primary, in: app.card) }
        if mode == "detached-name" { app.card.children.removeAll { $0 === app.nameNode }; app.list.children.append(app.nameNode) }
        if mode == "detached-primary" { app.card.children.removeAll { $0 === app.primary }; app.list.children.append(app.primary) }
        if mode == "detached-card" { app.list.children = []; app.route.children.append(app.card) }
        if mode == "detached-list" { app.route.children = []; app.children.append(app.list) }
        if mode == "duplicate-route" { app.children.append(XCUIElement("lb-secrets")) }
        if mode == "duplicate-list" { app.route.children.append(XCUIElement("lb-secrets-list")) }
        if mode == "wrong-primary-type" { app.primary.elementType = .staticText }
        if mode == "unrelated-same-name" {
            let other = XCUIElement("lb-secret-UNRELATED_OPAQUE_ID")
            other.children = [XCUIElement("lb-secret-name-UNRELATED_OPAQUE_ID", label: XCUIApplication.secretName)]
            app.list.children.append(other)
        }
        app.approveOriginalRequest()
        if mode == "reach" { app.fault = "reach"; app.reachable = false }
        if mode == "ready" { app.fault = "ready"; app.primary.isEnabled = false }
        if mode == "wrong-label" { app.primary.label = "Recover" }
        if mode == "after-ready-name" { app.afterReady = { app.nameNode.label = "RENAMED" } }
        if mode == "after-ready-id" { app.afterReady = { app.lbSelection.secretId = "OTHER_ID" } }
        if mode == "after-ready-disabled" { app.afterReady = { app.primary.isEnabled = false } }
        if mode == "after-ready-label" { app.afterReady = { app.primary.label = "Recover" } }
        if mode == "after-ready-primary" { app.afterReady = { app.duplicate(app.primary, in: app.card) } }
        if mode == "auth" || mode == "value" { app.fault = mode }
        if mode == "reentrant" { app.onShowInput = { app.tokenAtInput = owner.hasToken(); owner.driveShow(XCUIApplication.secretName, 1) } }
        if mode == "setup" { try! owner.setUpWithError(); tracked.append(owner.currentApp()) }
        if mode == "teardown" { try! owner.tearDownWithError() }
        if mode == "revealed-close" { app.disclosed.exists = true; owner.driveClose() }
        let pendingOnly = ["pending", "pending-delete", "pending-primary", "pending-duplicate", "pending-empty-id", "pending-map", "new-request-failed"]
        if !pendingOnly.contains(mode) { owner.driveShow(mode == "name" ? "OTHER_NAME" : XCUIApplication.secretName, mode == "cycle" ? 2 : 1) }
        if mode == "repeated" { owner.driveShow(XCUIApplication.secretName, 1) }
        if mode == "next-cycle" {
            owner.driveClose(); owner.driveRequest(XCUIApplication.secretName, 2); app.approveOriginalRequest()
            owner.driveShow(XCUIApplication.secretName, 2)
        }
        output[mode] = ["failures": AssertionPort.failures, "issued": issued, "token": owner.hasToken(),
            "lookups": tracked.reduce(0) { $0 + $1.lookups }, "exactReaches": tracked.reduce(0) { $0 + $1.exactReaches },
            "tapAttempts": tracked.reduce(0) { $0 + $1.tapAttempts }, "recoverAttempts": tracked.reduce(0) { $0 + $1.recoverAttempts },
            "showAttempts": tracked.reduce(0) { $0 + $1.showAttempts }, "recovers": app.recovers, "shows": app.shows,
            "pins": app.pinCalls, "valueRequires": app.valueRequires, "copyLookups": app.copyLookups,
            "firstFaultMatches": AssertionPort.primary === app.injectedFault,
            "tokenAtInput": app.tokenAtInput.map { $0 as Any } ?? NSNull()]
    }
    return output
}
@MainActor func inputCallbackCases() -> [String: Any] {
    var output: [String: Any] = [:]
    for mode in ["callback-allow", "callback-refuse", "callback-prior-fault"] {
        AssertionPort.reset()
        let app = XCUIApplication(); app.approveOriginalRequest()
        if mode == "callback-prior-fault" { AssertionPort.record("OWNED_CALLBACK_PRIMARY", app.injectedFault) }
        var callbacks = 0
        app.lbTap(app.primary.identifier, beforeInput: {
            callbacks += 1; app.events.append("callback")
            return mode != "callback-refuse"
        })
        output[mode] = ["callbacks": callbacks, "tapAttempts": app.tapAttempts, "showAttempts": app.showAttempts,
            "shows": app.shows, "recovers": app.recovers, "pins": app.pinCalls,
            "valueExists": app.disclosed.exists, "events": app.events, "failures": AssertionPort.failures,
            "firstFaultMatches": AssertionPort.primary === app.injectedFault]
    }
    return output
}
${scopeCases}
MainActor.assumeIsolated {
    let results = runCases().merging(matrixCases(), uniquingKeysWith: { first, _ in first })
        .merging(runScopeCases(), uniquingKeysWith: { first, _ in first })
        .merging(inputCallbackCases(), uniquingKeysWith: { first, _ in first })
    let data = try! JSONSerialization.data(withJSONObject: results, options: [.sortedKeys])
    print("SAME_REQUEST_FIXTURE " + String(decoding: data, as: UTF8.self))
}
`;
  const directory = mkdtempSync(resolve(tmpdir(), 'ms-r32-same-request-'));
  try {
    mkdirSync(resolve(directory, 'cache'));
    const path = resolve(directory, 'fixture.swift'); writeFileSync(path, fixture);
    const output = execFileSync('/usr/bin/swift', ['-module-cache-path', resolve(directory, 'cache'), path],
      { encoding: 'utf8', timeout: 25000 });
    for (const canary of ['CONTROLLED_OPAQUE_ID', 'SAME_REQUEST_NAME_CANARY', 'SAME_REQUEST_VALUE_CANARY', 'UNRELATED_OPAQUE_ID', 'OTHER_ID', 'OTHER_NAME', 'SNAPSHOT_ERROR_CANARY', 'f2-00000000-0000-4000-8000-000000000001']) {
      assert(!output.includes(canary), 'Captured actual-body output must not expose selector or value canaries');
    }
    const line = output.split('\n').find(value => value.startsWith('SAME_REQUEST_FIXTURE '));
    assert(line, 'Complete actual owner happy paths must finish before the semantic lookup assertion');
    hostResults = JSON.parse(line.slice('SAME_REQUEST_FIXTURE '.length)); return hostResults;
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

