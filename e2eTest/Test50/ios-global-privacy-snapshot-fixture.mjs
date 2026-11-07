import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

const owner = new URL('../../../meta-secret-compose/iosApp/iosAppUITests/LiveboardPrivacyTestActions.swift', import.meta.url);
function actualMethod(source, signature, optional = false) {
  const start = source.indexOf(signature);
  if (start < 0 && optional) return '';
  const end = source.indexOf('\n    }', start);
  assert(start >= 0 && end > start, 'Extract the complete actual owning privacy method');
  return source.slice(start, end + '\n    }'.length);
}
let results;
export function actualGlobalPrivacyResults() {
  if (results) return results;
  const source = readFileSync(owner, 'utf8');
  const method = actualMethod(source, '    func lbAssertNoPrivateContent(');
  const collector = actualMethod(source, '    private func lbGlobalPrivacySnapshot(', true);
  // No scan/predicate/assertion implementation is copied: OLD body runs without a collector.
  const fixture = String.raw`import Foundation
import CoreGraphics
@MainActor enum PrivacyPort {
    static let expected = "GLOBAL_PRIVATE_VALUE_CANARY"
    static let identifierCanary = "GLOBAL_PRIVATE_IDENTIFIER_CANARY"
    static var failures: [String] = [], assertions = 0, captures = 0
    static var liveExists = 0, liveLabels = 0, liveValues = 0, liveIdentifiers = 0, liveQueries = 0
    static var snapshotReads = 0, snapshotNodes = Set<Int>(), nextNode = 0, childrenReads = 0
    static var actions = 0, clipboard = 0, auth = 0, navigation = 0, clockReads = 0
    static var elapsed: TimeInterval = 0, mode = "", captureThrows = false
    static func reset(_ name: String) {
        failures = []; assertions = 0; captures = 0; liveExists = 0; liveLabels = 0; liveValues = 0
        liveIdentifiers = 0; liveQueries = 0; snapshotReads = 0; snapshotNodes = []; nextNode = 0
        childrenReads = 0; actions = 0; clipboard = 0; auth = 0; navigation = 0; clockReads = 0
        elapsed = 0; mode = name; captureThrows = false
    }
    static func record(_ message: String) { failures.append(message) }
}
@MainActor func XCTFail(_ message: String = "") { PrivacyPort.record(message.isEmpty ? "FAIL" : message) }
@MainActor func XCTAssertFalse(_ condition: Bool, _ message: String = "") {
    PrivacyPort.assertions += 1; if condition { XCTFail(message) }
}
@MainActor func XCTAssertEqual<T: Equatable>(_ first: T, _ second: T, _ message: String = "") {
    PrivacyPort.assertions += 1; if first != second { XCTFail(message.isEmpty ? "EQUAL" : message) }
}
@MainActor final class HostProcessInfo {
    var systemUptime: TimeInterval { PrivacyPort.clockReads += 1; return PrivacyPort.elapsed }
}
@MainActor enum ProcessInfo { static let processInfo = HostProcessInfo() }
@MainActor protocol XCUIElementSnapshot {
    var identifier: String { get }
    var label: String { get }
    var value: Any? { get }
    var children: [any XCUIElementSnapshot] { get }
    var frame: CGRect { get }
    var elementType: HostElementType { get }
}
enum HostElementType { case any, application, window, other }
@MainActor class XCUIElement {
    var rawIdentifier: String, rawLabel: String, rawValue: Any?, rawExists = true
    var rawChildren: [XCUIElement] = []
    var rawFrame = CGRect(x: 0, y: 0, width: 300, height: 500), rawType = HostElementType.other
    init(_ identifier: String = "ordinary", label: String = "ordinary", value: Any? = nil) {
        rawIdentifier = identifier; rawLabel = label; rawValue = value
    }
    var identifier: String { PrivacyPort.liveIdentifiers += 1; return rawIdentifier }
    var label: String { PrivacyPort.liveLabels += 1; return rawLabel }
    var value: Any? { PrivacyPort.liveValues += 1; return rawValue }
    var exists: Bool { PrivacyPort.liveExists += 1; return rawExists }
    var frame: CGRect { rawFrame }
    var elementType: HostElementType { rawType }
    func descendants(matching type: HostElementType) -> HostQuery {
        PrivacyPort.liveQueries += 1
        var pending = rawChildren, found: [XCUIElement] = []
        while let node = pending.popLast() { found.append(node); pending.append(contentsOf: node.rawChildren) }
        return HostQuery(found)
    }
    func tap() { PrivacyPort.actions += 1 }
    func typeText(_ text: String) { PrivacyPort.actions += 1 }
    func press(forDuration duration: TimeInterval) { PrivacyPort.actions += 1 }
}
@MainActor final class HostQuery {
    let nodes: [XCUIElement]
    init(_ nodes: [XCUIElement]) { self.nodes = nodes }
    var allElementsBoundByIndex: [XCUIElement] { nodes.filter(\.rawExists) }
    var count: Int { allElementsBoundByIndex.count }
    func matching(_ predicate: NSPredicate) -> HostQuery {
        HostQuery(nodes.filter { predicate.evaluate(with: ["identifier": $0.identifier]) })
    }
}
@MainActor final class FrozenNode: XCUIElementSnapshot {
    let index: Int, root: Bool, storedIdentifier: String, storedLabel: String, storedValue: Any?
    let storedChildren: [FrozenNode], storedFrame: CGRect, storedType: HostElementType
    init(_ live: XCUIElement, root: Bool = false) {
        PrivacyPort.nextNode += 1; index = PrivacyPort.nextNode; self.root = root
        storedIdentifier = live.rawIdentifier; storedLabel = live.rawLabel; storedValue = live.rawValue
        storedFrame = live.rawFrame; storedType = live.rawType
        storedChildren = live.rawChildren.map { FrozenNode($0) }
    }
    private func read() {
        PrivacyPort.snapshotReads += 1
        if !root { PrivacyPort.snapshotNodes.insert(index) }
    }
    var identifier: String {
        read(); if !root && PrivacyPort.mode == "walk-expired" { PrivacyPort.elapsed = 16 }
        return storedIdentifier
    }
    var label: String {
        read(); if !root && PrivacyPort.mode == "attribute-late" { PrivacyPort.elapsed = 16 }
        return storedLabel
    }
    var value: Any? { read(); return storedValue }
    var frame: CGRect { read(); return storedFrame }
    var elementType: HostElementType { read(); return storedType }
    var children: [any XCUIElementSnapshot] {
        read(); PrivacyPort.childrenReads += 1
        if !root && storedChildren.isEmpty && PrivacyPort.mode == "postwalk-late" { PrivacyPort.elapsed = 16 }
        if root && PrivacyPort.mode == "capture-spent-budget" { PrivacyPort.elapsed += 2 }
        return storedChildren
    }
}
@MainActor final class HostPasteboard {
    var string: String? {
        get { PrivacyPort.clipboard += 1; return nil }
        set { PrivacyPort.clipboard += 1 }
    }
}
@MainActor enum UIPasteboard { static let general = HostPasteboard() }
@MainActor final class XCUIApplication: XCUIElement {
    var afterCapture: (() -> Void)?
    func snapshot() throws -> any XCUIElementSnapshot {
        PrivacyPort.captures += 1
        if PrivacyPort.captureThrows {
            throw NSError(domain: "CAPTURE_ERROR_CANARY", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "CAPTURE_ERROR_BODY_CANARY"])
        }
        let frozen = FrozenNode(self, root: true)
        if PrivacyPort.mode == "capture-late" { PrivacyPort.elapsed = 16 }
        if PrivacyPort.mode == "capture-spent-budget" { PrivacyPort.elapsed += 14 }
        if PrivacyPort.mode == "under-deadline" { PrivacyPort.elapsed = 14.999 }
        let callback = afterCapture; afterCapture = nil; callback?()
        return frozen
    }
    func lbConfirmSystemIfPresent() -> Bool { PrivacyPort.auth += 1; return false }
    func lbSecrets() { PrivacyPort.navigation += 1 }
    func launch() { PrivacyPort.actions += 1 }
    func terminate() { PrivacyPort.actions += 1 }
${method}
${collector}
}
@MainActor func runPrivacyCases() -> [String: Any] {
    let modes = ["clean", "nested", "empty", "label-leak", "value-leak", "value-prefix", "copy-prefix",
        "both-prefixes", "root-only", "nil-value", "non-string-value", "case-sensitive", "near-prefix",
        "other-window-label", "offscreen-value", "nested-value", "duplicate-private", "limit-8192",
        "limit-8193", "large-frontier", "capture-throw", "capture-unavailable", "capture-incomplete", "capture-late",
        "capture-spent-budget", "walk-expired", "attribute-late", "postwalk-late", "under-deadline",
        "two-clean-calls", "fresh-two-calls", "immutable-clean", "immutable-leak", "snapshot-presence"]
    var output: [String: Any] = [:]
    for mode in modes {
        PrivacyPort.reset(mode)
        let app = XCUIApplication("application", label: "application")
        app.rawType = .application
        let ordinary = XCUIElement(PrivacyPort.identifierCanary)
        app.rawChildren = [ordinary]
        switch mode {
        case "empty": app.rawChildren = []
        case "nested", "nested-value":
            let parent = XCUIElement("window-one"), child = XCUIElement("nested-container")
            if mode == "nested-value" { ordinary.rawValue = "prefix-" + PrivacyPort.expected + "-suffix" }
            child.rawChildren = [ordinary]; parent.rawChildren = [child]; app.rawChildren = [parent]
        case "label-leak": ordinary.rawLabel = "prefix-" + PrivacyPort.expected + "-suffix"
        case "value-leak": ordinary.rawValue = "prefix-" + PrivacyPort.expected + "-suffix"
        case "value-prefix": ordinary.rawIdentifier = "lb-value-" + PrivacyPort.identifierCanary
        case "copy-prefix": ordinary.rawIdentifier = "lb-copy-" + PrivacyPort.identifierCanary
        case "both-prefixes", "duplicate-private":
            ordinary.rawIdentifier = "lb-value-" + PrivacyPort.identifierCanary
            let copy = XCUIElement("lb-copy-" + PrivacyPort.identifierCanary)
            app.rawChildren.append(copy)
            if mode == "duplicate-private" { app.rawChildren.append(XCUIElement(ordinary.rawIdentifier)) }
        case "root-only":
            app.rawLabel = PrivacyPort.expected; app.rawValue = PrivacyPort.expected; app.rawIdentifier = "lb-value-root"
        case "nil-value": ordinary.rawValue = nil
        case "non-string-value": ordinary.rawValue = 42
        case "case-sensitive": ordinary.rawLabel = PrivacyPort.expected.lowercased()
        case "near-prefix": ordinary.rawIdentifier = "x-lb-value-" + PrivacyPort.identifierCanary
        case "other-window-label", "offscreen-value":
            let first = XCUIElement("window-one"), second = XCUIElement("window-two")
            first.rawType = .window; second.rawType = .window
            first.rawChildren = [ordinary]
            let hidden = XCUIElement("offscreen-descendant")
            hidden.rawFrame = CGRect(x: 0, y: 900, width: 100, height: 30)
            if mode == "other-window-label" { hidden.rawLabel = PrivacyPort.expected }
            else { hidden.rawValue = PrivacyPort.expected }
            second.rawChildren = [hidden]; app.rawChildren = [first, second]
        case "limit-8192", "limit-8193", "large-frontier":
            let count = mode == "limit-8192" ? 8192 : (mode == "limit-8193" ? 8193 : 20000)
            app.rawChildren = (0..<count).map { _ in XCUIElement() }
            if mode == "large-frontier" { app.rawChildren.last?.rawValue = PrivacyPort.expected }
        case "capture-throw", "capture-unavailable", "capture-incomplete": PrivacyPort.captureThrows = true
        case "immutable-clean": app.afterCapture = { ordinary.rawLabel = PrivacyPort.expected }
        case "immutable-leak":
            ordinary.rawValue = PrivacyPort.expected; app.afterCapture = { ordinary.rawValue = nil }
        case "snapshot-presence": ordinary.rawExists = false; ordinary.rawLabel = PrivacyPort.expected
        default: break
        }
        app.lbAssertNoPrivateContent(PrivacyPort.expected)
        let firstFailures = PrivacyPort.failures.count, firstAssertions = PrivacyPort.assertions
        if ["two-clean-calls", "fresh-two-calls"].contains(mode) {
            if mode == "fresh-two-calls" { ordinary.rawLabel = PrivacyPort.expected }
            app.lbAssertNoPrivateContent(PrivacyPort.expected)
        }
        output[mode] = ["failures": PrivacyPort.failures, "assertions": PrivacyPort.assertions,
            "firstFailures": firstFailures, "firstAssertions": firstAssertions, "captures": PrivacyPort.captures,
            "liveExists": PrivacyPort.liveExists, "liveLabels": PrivacyPort.liveLabels,
            "liveValues": PrivacyPort.liveValues, "liveIdentifiers": PrivacyPort.liveIdentifiers,
            "liveQueries": PrivacyPort.liveQueries, "snapshotReads": PrivacyPort.snapshotReads,
            "snapshotNodes": PrivacyPort.snapshotNodes.count, "childrenReads": PrivacyPort.childrenReads,
            "actions": PrivacyPort.actions, "clipboard": PrivacyPort.clipboard, "auth": PrivacyPort.auth,
            "navigation": PrivacyPort.navigation, "clockReads": PrivacyPort.clockReads]
    }
    return output
}
MainActor.assumeIsolated {
    let data = try! JSONSerialization.data(withJSONObject: runPrivacyCases(), options: [.sortedKeys])
    print("GLOBAL_PRIVACY_FIXTURE " + String(decoding: data, as: UTF8.self))
}
`;
  const directory = mkdtempSync(resolve(tmpdir(), 'ms-r32-global-privacy-'));
  try {
    mkdirSync(resolve(directory, 'cache'));
    const path = resolve(directory, 'fixture.swift'); writeFileSync(path, fixture);
    const execution = spawnSync('/usr/bin/swift', ['-module-cache-path', resolve(directory, 'cache'), path],
      { encoding: 'utf8', timeout: 30000 });
    const output = execution.stdout ?? '', diagnostics = execution.stderr ?? '';
    for (const canary of ['GLOBAL_PRIVATE_VALUE_CANARY', 'GLOBAL_PRIVATE_IDENTIFIER_CANARY',
      'CAPTURE_ERROR_CANARY', 'CAPTURE_ERROR_BODY_CANARY', 'offscreen-descendant', 'lb-value-root']) {
      assert(!(output + diagnostics).includes(canary), 'Actual-body output must not disclose payload, identifiers or capture errors');
    }
    assert.equal(execution.error, undefined, 'The Root-owned actual-source Swift invocation must finish');
    assert.equal(execution.status, 0, `Actual-source Swift compile/run must succeed: ${diagnostics.slice(0, 1800)}`);
    const lines = output.split('\n').filter(line => line.startsWith('GLOBAL_PRIVACY_FIXTURE '));
    assert.equal(lines.length, 1, 'The actual owning body must complete the closed matrix exactly once');
    results = JSON.parse(lines[0].slice('GLOBAL_PRIVACY_FIXTURE '.length)); return results;
  } finally { rmSync(directory, { recursive: true, force: true }); }
}
