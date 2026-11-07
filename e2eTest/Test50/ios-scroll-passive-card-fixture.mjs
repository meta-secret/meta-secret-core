import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';

// Whole owning bodies only. Closed AX/gesture ports supply observations, never reachability decisions.
export function buildPassiveCardFixture(source, appSource) {
  const declarations = source.slice(source.indexOf('func liveboardViewportKey('), source.indexOf('\n@MainActor'));
  const methods = source.slice(source.indexOf('    @discardableResult\n    func lbFindInList('), (source.indexOf('    func lbFillCreate(') >= 0 ? source.indexOf('    func lbFillCreate(') : source.indexOf('    func lbHide(')));
  const snapshot = appSource.slice(appSource.indexOf('    func lbSecretsViewportSnapshot()'), appSource.indexOf('    func lbWaitMain('));
  assert(methods.startsWith('    @discardableResult\n    func lbFindInList('));
  assert(methods.includes('private func lbTraverseSecrets('));
  assert(snapshot.startsWith('    func lbSecretsViewportSnapshot()'));
  return String.raw`import Foundation
import CoreGraphics
@MainActor enum AssertionPort { static var failures: [String] = [] }
@MainActor func XCTFail(_ message: String) { AssertionPort.failures.append(message) }
@MainActor func XCTAssertTrue(_ value: Bool, _ message: String = "") { if !value { XCTFail(message) } }
@MainActor func XCTAssertEqual<T: Equatable>(_ a: T, _ b: T, _ message: String = "") { if a != b { XCTFail(message) } }
@MainActor func XCTAssertLessThanOrEqual<T: Comparable>(_ a: T, _ b: T, _ message: String = "") { if a > b { XCTFail(message) } }
enum HostElementType { case window, other, button }
protocol XCUIElementSnapshot {
    var identifier: String { get }; var label: String { get }; var frame: CGRect { get }
    var elementType: HostElementType { get }; var children: [any XCUIElementSnapshot] { get }
}
final class SnapshotNode: XCUIElementSnapshot {
    let identifier: String, label: String, frame: CGRect, elementType: HostElementType
    let children: [any XCUIElementSnapshot]
    init(_ id: String, _ frame: CGRect, _ children: [any XCUIElementSnapshot] = [], window: Bool = false) {
        identifier = id; label = id.hasPrefix("lb-secret-name-") ? "Synthetic" : ""
        self.frame = frame; self.children = children; elementType = window ? .window : .other
    }
}
@MainActor final class SelectionPort { var secrets: [String: String] = [:] }
@MainActor class XCUIElement {
    let identifier: String
    weak var owner: XCUIApplication?
    var exists = true, visible = true, isHittable = false
    init(_ id: String = "", owner: XCUIApplication? = nil) { identifier = id; self.owner = owner }
    var frame: CGRect { owner?.liveFrameReads += 1; return CGRect(x: 0, y: 0, width: 400, height: 600) }
    func swipeDown() { owner?.rawGestures += 1 }
    func swipeUp() { owner?.rawGestures += 1 }
    func tap() { owner?.actions += 1 }
}
@MainActor final class HostCoordinate {
    let app: XCUIApplication
    init(_ app: XCUIApplication) { self.app = app }
    func withOffset(_ value: CGVector) -> HostCoordinate { self }
    func press(forDuration: TimeInterval, thenDragTo: HostCoordinate) { app.drags += 1 }
}
@MainActor final class XCUIApplication: XCUIElement {
    let scenario: String
    var snapshots = 0, drags = 0, rawGestures = 0, actions = 0, liveFrameReads = 0, visits = 0, observations = 0
    var visibleChecks = 0, secretsEntries = 0
    let lbSelection = SelectionPort()
    lazy var target = XCUIElement(scenario.hasPrefix("action-") ? "lb-primary-one" : "lb-secret-one", owner: self)
    init(_ scenario: String) { self.scenario = scenario; super.init() }
    func lbSecrets(timeout: TimeInterval = 180) { secretsEntries += 1 }
    func lbElement(_ identifier: String) -> XCUIElement {
        XCTAssertEqual(identifier, target.identifier, "Exact selector port"); return target
    }
    func lbRequire(_ identifier: String, timeout: TimeInterval = 30) -> XCUIElement {
        XCTAssertEqual(identifier, "lb-secrets-list", "Only current list may be required"); return XCUIElement(identifier)
    }
    func lbVisible(_ element: XCUIElement) -> Bool { visibleChecks += 1; return element.exists && element.visible }
    func coordinate(withNormalizedOffset: CGVector) -> HostCoordinate { HostCoordinate(self) }
    func snapshot() throws -> any XCUIElementSnapshot {
        snapshots += 1
        if scenario == "snapshot-error" { throw HostError.snapshot }
        let full = CGRect(x: 0, y: 0, width: 400, height: 600)
        var rootFrame = full, windowFrame = full, routeFrame = full, listFrame = full
        if scenario == "nonfinite-root" { rootFrame.origin.x = .infinity }
        if scenario == "nonfinite-window" { windowFrame.origin.x = .infinity }
        if scenario == "nonfinite-route" { routeFrame.origin.x = .infinity }
        if scenario == "nonfinite-list" { listFrame.origin.x = .infinity }
        if scenario == "empty-clip" { listFrame = CGRect(x: 900, y: 900, width: 1, height: 1) }
        var cardFrame = CGRect(x: 20, y: 200, width: 360, height: 150)
        var nameFrame = CGRect(x: 30, y: 220, width: 300, height: 30)
        if scenario == "offclip-card" { cardFrame.origin.y = 900 }
        if scenario == "offclip-name" { nameFrame.origin.y = 900 }
        if scenario == "nonfinite-card" { cardFrame.origin.x = .nan }
        if scenario == "nonfinite-name" { nameFrame.origin.x = .nan }
        if scenario == "empty-card" { cardFrame.size.width = 0 }
        if scenario == "empty-name" { nameFrame.size.height = 0 }
        if scenario == "partial-card" { cardFrame.origin.y = -20 }
        let card = SnapshotNode(scenario == "wrong-id" ? "lb-secret-other" : "lb-secret-one", cardFrame)
        let name = SnapshotNode(scenario == "wrong-name-id" ? "lb-secret-name-other" : "lb-secret-name-one", nameFrame)
        var targets: [any XCUIElementSnapshot] = []
        if scenario != "missing-card" { targets.append(card) }
        if scenario != "missing-name" { targets.append(name) }
        if scenario == "duplicate-card" { targets.append(card) }
        if scenario == "duplicate-name" { targets.append(name) }
        // Additional real cards/names retain stable list keys even when the selected title is absent.
        targets.append(SnapshotNode("lb-secret-two", CGRect(x: 20, y: 20, width: 360, height: 150)))
        targets.append(SnapshotNode("lb-secret-name-two", CGRect(x: 30, y: 40, width: 300, height: 30)))
        if scenario != "passive-two" {
            targets.append(SnapshotNode("lb-secret-three", CGRect(x: 20, y: 380, width: 360, height: 150)))
            targets.append(SnapshotNode("lb-secret-name-three", CGRect(x: 30, y: 400, width: 300, height: 30)))
        }
        if scenario == "node-budget" { targets += (0..<8193).map { _ in SnapshotNode("", full) } }
        let list = SnapshotNode("lb-secrets-list", listFrame)
        let route = SnapshotNode("lb-secrets", routeFrame)
        var nodes: [any XCUIElementSnapshot] = targets
        if scenario != "missing-route" { nodes.append(route) }
        if scenario != "missing-list" { nodes.append(list) }
        if scenario == "duplicate-route" { nodes.append(route) }
        if scenario == "duplicate-list" { nodes.append(list) }
        var foreign: [any XCUIElementSnapshot] = []
        if ["foreign-card", "foreign-name"].contains(scenario) {
            let id = scenario == "foreign-card" ? card.identifier : name.identifier
            nodes.removeAll { $0.identifier == id }
            foreign = [scenario == "foreign-card" ? card : name]
        }
        if scenario == "equal-frame-foreign-list" { nodes.removeAll { $0.identifier == list.identifier }; foreign = [list] }
        if scenario == "equal-frame-foreign-route" { nodes.removeAll { $0.identifier == route.identifier }; foreign = [route] }
        if scenario == "missing-window" { return SnapshotNode("", rootFrame, nodes) }
        var windows: [any XCUIElementSnapshot] = [SnapshotNode("", windowFrame, nodes, window: true)]
        if !foreign.isEmpty { windows.append(SnapshotNode("", windowFrame, foreign, window: true)) }
        return SnapshotNode("", rootFrame, windows)
    }
    func exercise(_ mode: String) -> Bool {
        target.isHittable = scenario == "action-hittable"
        if mode == "find" { return lbFindInList(target.identifier) }
        return lbTraverseSecrets(target: target) { _ in observations += 1 }
    }
}
enum HostError: Error { case snapshot }
${declarations}
@MainActor extension XCUIApplication {
${snapshot}
${methods}
}
MainActor.assumeIsolated {
    let scenarios = ["passive-two", "passive-three", "partial-card", "wrong-id", "wrong-name-id", "missing-card", "missing-name",
        "duplicate-card", "duplicate-name", "offclip-card", "offclip-name", "nonfinite-card", "nonfinite-name", "empty-card", "empty-name",
        "missing-route", "missing-list", "duplicate-route", "duplicate-list", "missing-window", "foreign-card", "foreign-name",
        "equal-frame-foreign-list", "equal-frame-foreign-route", "nonfinite-root", "nonfinite-window", "nonfinite-route", "nonfinite-list",
        "empty-clip", "node-budget", "snapshot-error", "action-unhittable", "action-hittable"]
    var results: [String: Any] = [:]
    for mode in ["traverse", "find"] {
        for scenario in scenarios {
            AssertionPort.failures = []
            let app = XCUIApplication(scenario)
            let result = app.exercise(mode)
            results[mode + ":" + scenario] = ["result": result, "snapshots": app.snapshots, "drags": app.drags,
                "raw": app.rawGestures, "actions": app.actions, "liveFrameReads": app.liveFrameReads,
                "observations": app.observations, "visibleChecks": app.visibleChecks, "targetExists": app.target.exists,
                "targetHittable": app.target.isHittable, "failures": AssertionPort.failures]
        }
    }
    let data = try! JSONSerialization.data(withJSONObject: results, options: [.sortedKeys])
    print("PASSIVE_CARD_FIXTURE " + String(decoding: data, as: UTF8.self))
}
`;
}

export function runPassiveCardFixture(source, appSource) {
  const root = mkdtempSync(resolve(tmpdir(), 'ms-r32-passive-card-')); mkdirSync(resolve(root, 'cache'));
  try {
    const path = resolve(root, 'fixture.swift'); writeFileSync(path, buildPassiveCardFixture(source, appSource));
    const output = execFileSync('/usr/bin/swift', ['-module-cache-path', resolve(root, 'cache'), path], { encoding: 'utf8', timeout: 25000 });
    const line = output.split('\n').find(value => value.startsWith('PASSIVE_CARD_FIXTURE '));
    assert(line, 'Complete actual methods must finish before semantic assertions');
    return JSON.parse(line.slice('PASSIVE_CARD_FIXTURE '.length));
  } finally { rmSync(root, { recursive: true, force: true }); }
}
