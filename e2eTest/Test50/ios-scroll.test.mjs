import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
const source = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/LiveboardSecretTestActions.swift', import.meta.url), 'utf8');
const appSource = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/LiveboardTestActions.swift', import.meta.url), 'utf8');

test('iOS search guarantees independent up/down budgets and absence requires both proven ends', () => {
  const search = source.slice(source.indexOf('    private func lbTraverseSecrets('), (source.indexOf('    func lbFillCreate(') >= 0 ? source.indexOf('    func lbFillCreate(') : source.indexOf('    func lbHide(')));
  assert(search.includes('for upward in [true, false]'));
  assert(search.indexOf('let directionDeadline = Date().addingTimeInterval(60)') > search.indexOf('for upward in [true, false]'));
  assert(search.includes('for _ in 0..<64'));
  assert(search.includes('unchanged >= 3'));
  assert(search.includes('reachedBothEnds = reachedBothEnds && unchanged >= 3'));
  assert(search.includes('XCTAssertTrue(reachedBothEnds'));
  assert(!search.includes('map { "\\($0.identifier):\\($0.frame)" }'));
  assert(search.includes('let (root, bounds) = try lbSecretsViewportSnapshot()')); assert(search.includes('visited <= 8192'));
  const scoped = appSource.slice(appSource.indexOf('func lbSecretsViewportSnapshot()'), appSource.indexOf('func lbWaitMain('));
  assert.equal((scoped.match(/try snapshot\(\)/g) ?? []).length, 1);
  for (const guard of ['visited <= 8192', 'routeCount == 1, listCount == 1', 'route.window == list.window',
    'list.frame.intersection(route.frame).intersection(list.window).intersection(root.frame)', 'guard finite(bounds)', 'return (root, bounds)']) assert(scoped.includes(guard));
  assert(search.includes('node.frame.intersects(bounds)'));
  assert(search.includes('for attempt in 1...3')); assert(search.includes('Date().addingTimeInterval(15)'));
  assert(search.includes('liveboardViewportKey(id: $0.identifier, row: $0.frame, viewport: bounds)'));
});

test('actual Swift viewport key ignores elastic bounce while retaining clipping and identity changes', { timeout: 30000 }, () => {
  const start = source.indexOf('func liveboardViewportKey(');
  const end = source.indexOf('\n}\n', start) + 3;
  assert(start >= 0 && end > start);
  const method = source.slice(start, end);
  assert(source.includes('liveboardViewportKey(id: $0.identifier, row: $0.frame, viewport: bounds)'));
  const root = mkdtempSync(resolve(tmpdir(), 'ms96-viewport-regression-')); mkdirSync(resolve(root, 'cache'));
  try {
    const fixture = `import Foundation\nimport CoreGraphics\n${method}\nlet viewport = CGRect(x: 0, y: 0, width: 200, height: 400)\nlet row = CGRect(x: 0, y: 0, width: 200, height: 100)\nlet base = liveboardViewportKey(id: "row", row: row, viewport: viewport)\nfor delta in [-3.0, -1.0, 0.0, 1.0, 3.0] { precondition(liveboardViewportKey(id: "row", row: row.offsetBy(dx: 0, dy: delta), viewport: viewport) == base) }\nprecondition(liveboardViewportKey(id: "other-row", row: row, viewport: viewport) != base)\nprecondition(liveboardViewportKey(id: "row", row: row.offsetBy(dx: 0, dy: -30), viewport: viewport) != base)\nprint("viewport-key-regression-pass")\n`;
    const path = resolve(root, 'fixture.swift'); writeFileSync(path, fixture);
    const output = execFileSync('/usr/bin/swift', ['-module-cache-path', resolve(root, 'cache'), path], { encoding: 'utf8', timeout: 25000 });
    assert(output.includes('viewport-key-regression-pass'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

// Compile the complete owning methods; only XCTest/AX/input delivery are closed ports.
// Root alone runs this fixture: node --check never invokes the Swift child below.
let traversalHostResults;
function actualTraversalResults() {
  if (traversalHostResults) return traversalHostResults;
  const declarations = source.slice(source.indexOf('func liveboardViewportKey('), source.indexOf('\n@MainActor'));
  const reachability = source.indexOf('    private func lbSecretTargetReachable(');
  const traversal = source.slice(reachability >= 0 ? reachability : source.indexOf('    @discardableResult\n    private func lbTraverseSecrets('), (source.indexOf('    func lbFillCreate(') >= 0 ? source.indexOf('    func lbFillCreate(') : source.indexOf('    func lbHide(')));
  const scopedSnapshot = appSource.slice(appSource.indexOf('    func lbSecretsViewportSnapshot()'), appSource.indexOf('    func lbWaitMain('));
  assert(declarations.includes('private struct LiveboardSecretViewportNode'));
  assert(traversal.includes('    @discardableResult\n    private func lbTraverseSecrets('));
  assert(scopedSnapshot.startsWith('    func lbSecretsViewportSnapshot()'));
  const fixture = String.raw`import Foundation
import CoreGraphics

@MainActor enum AssertionPort { static var failures: [String] = [] }
@MainActor func XCTFail(_ message: String) { AssertionPort.failures.append(message) }
@MainActor func XCTAssertTrue(_ value: Bool, _ message: String = "") {
    if !value { XCTFail(message) }
}
@MainActor func XCTAssertEqual<T: Equatable>(_ left: T, _ right: T, _ message: String = "") {
    if left != right { XCTFail(message) }
}
enum HostElementType { case window, other }
protocol XCUIElementSnapshot {
    var identifier: String { get }
    var label: String { get }
    var frame: CGRect { get }
    var elementType: HostElementType { get }
    var children: [any XCUIElementSnapshot] { get }
}
final class SnapshotNode: XCUIElementSnapshot {
    let identifier: String, label: String
    let frame: CGRect
    let elementType: HostElementType
    let children: [any XCUIElementSnapshot]
    init(_ identifier: String = "", _ frame: CGRect, label: String = "", window: Bool = false,
         children: [any XCUIElementSnapshot] = []) {
        self.identifier = identifier; self.frame = frame; self.label = label
        self.elementType = window ? .window : .other; self.children = children
    }
}
@MainActor class XCUIElement {
    weak var owner: XCUIApplication?
    var identifier = ""
    var visible = true, exists = true, isHittable = true
    var frame: CGRect {
        owner?.liveFrameReads += 1
        return CGRect(x: -4000, y: -5000, width: 8000, height: 9000)
    }
    init(owner: XCUIApplication? = nil) { self.owner = owner }
    func swipeDown() { owner?.rawGesture("down") }
    func swipeUp() { owner?.rawGesture("up") }
}
@MainActor final class HostCoordinate {
    let app: XCUIApplication, offset: CGVector
    init(_ app: XCUIApplication, _ offset: CGVector = .zero) { self.app = app; self.offset = offset }
    func withOffset(_ value: CGVector) -> HostCoordinate {
        HostCoordinate(app, CGVector(dx: offset.dx + value.dx, dy: offset.dy + value.dy))
    }
    func press(forDuration duration: TimeInterval, thenDragTo end: HostCoordinate) {
        func number(_ value: CGFloat) -> Any { value.isFinite ? Double(value) : "NONFINITE" }
        app.drags.append(["start": [number(offset.dx), number(offset.dy)],
                          "end": [number(end.offset.dx), number(end.offset.dy)],
                          "duration": duration, "snapshot": app.snapshotCalls])
        app.cursor += 1
    }
}
@MainActor final class XCUIApplication: XCUIElement {
    let scenario: String
    var cursor = 0, snapshotCalls = 0, liveFrameReads = 0
    var raw: [String] = [], drags: [[String: Any]] = [], origins: [[Double]] = []
    var observations: [[String]] = []
    lazy var list = XCUIElement(owner: self)
    init(_ scenario: String) { self.scenario = scenario; super.init() }
    override var frame: CGRect {
        liveFrameReads += 1
        return CGRect(x: 800, y: 900, width: 30, height: 40)
    }
    func lbRequire(_ identifier: String, timeout: TimeInterval = 30) -> XCUIElement {
        XCTAssertEqual(identifier, "lb-secrets-list", "Unexpected require port")
        return list
    }
    func lbVisible(_ element: XCUIElement) -> Bool { element.exists && element.visible }
    func rawGesture(_ direction: String) { raw.append(direction); cursor += 1 }
    func coordinate(withNormalizedOffset value: CGVector) -> HostCoordinate {
        origins.append([Double(value.dx), Double(value.dy)])
        return HostCoordinate(self)
    }
    func snapshot() throws -> any XCUIElementSnapshot {
        snapshotCalls += 1
        let shifted = scenario == "snapshot-binding" && snapshotCalls % 2 == 0
        var root = shifted ? CGRect(x: 130, y: 240, width: 360, height: 600) : CGRect(x: 30, y: 40, width: 300, height: 500)
        var window = shifted ? CGRect(x: 140, y: 250, width: 320, height: 550) : CGRect(x: 40, y: 50, width: 260, height: 450)
        var route = shifted ? CGRect(x: 150, y: 260, width: 280, height: 450) : CGRect(x: 50, y: 60, width: 240, height: 400)
        var listFrame = CGRect(x: -1000, y: -2000, width: 4400, height: 6400)
        if scenario == "nonfinite-root" { root.origin.x = .infinity }
        if scenario == "nonfinite-window" { window.origin.y = .infinity }
        if scenario == "nonfinite-route" { route.size.width = .nan }
        if scenario == "nonfinite-list" { listFrame.origin.x = .infinity }
        if scenario == "empty-root" { root.size.width = 0 }
        if scenario == "empty-clip" { listFrame = CGRect(x: 2000, y: 2000, width: 10, height: 10) }
        let identifier = scenario == "identity-change" && cursor > 0 ? "lb-secret-name-next" : "lb-secret-name-one"
        var row = shifted ? CGRect(x: 160, y: 270, width: 150, height: 20) : CGRect(x: 60, y: 70, width: 150, height: 20)
        if scenario == "clipping-change" && cursor > 0 { row.origin.y = 45 }
        if scenario == "bounce" { row.origin.y += [-3.0, -1.0, 0.0, 1.0, 3.0][cursor % 5] }
        var rows: [any XCUIElementSnapshot] = [SnapshotNode(identifier, row, label: "Synthetic")]
        if scenario == "duplicate" { rows.append(SnapshotNode(identifier, row, label: "Synthetic")) }
        if scenario == "empty" { rows = [SnapshotNode("", row, label: "No secrets")] }
        if scenario == "unproven-empty" { rows = [] }
        if scenario == "node-budget" { rows = (0..<8193).map { _ in SnapshotNode("", row) } }
        let listNode = SnapshotNode("lb-secrets-list", listFrame, children: rows)
        var lists: [any XCUIElementSnapshot] = scenario == "missing-list" ? [] : [listNode]
        if scenario == "ambiguous-list" { lists.append(listNode) }
        let routeNode = SnapshotNode("lb-secrets", route, children: lists)
        if scenario == "missing-window" { return SnapshotNode("", root, children: [routeNode]) }
        if scenario == "wrong-window" {
            return SnapshotNode("", root, children: [
                SnapshotNode("", window, window: true, children: [SnapshotNode("lb-secrets", route)]),
                SnapshotNode("", CGRect(x: 45, y: 55, width: 250, height: 440), window: true, children: [listNode])])
        }
        var routes: [any XCUIElementSnapshot] = scenario == "missing-route" ? [listNode] : [routeNode]
        if scenario == "ambiguous-route" { routes.append(SnapshotNode("lb-secrets", route)) }
        return SnapshotNode("", root, children: [SnapshotNode("", window, window: true, children: routes)])
    }
}

${declarations}
@MainActor extension XCUIApplication {
${scopedSnapshot}
${traversal}
    func exercise(target: XCUIElement?) -> Bool {
        lbTraverseSecrets(target: target) { rows in observations.append(rows.map(\.identifier)) }
    }
}
@MainActor func runTraversalCases() -> [String: Any] {
    var results: [String: Any] = [:]
    let names = ["stable", "snapshot-binding", "identity-change", "clipping-change", "bounce", "empty", "unproven-empty",
                 "target", "unhittable-target", "duplicate", "missing-window", "wrong-window", "ambiguous-route",
                 "ambiguous-list", "missing-route", "missing-list", "nonfinite-root", "nonfinite-window",
                 "nonfinite-route", "nonfinite-list", "empty-root", "empty-clip", "node-budget"]
    for name in names {
        AssertionPort.failures = []
        let app = XCUIApplication(name)
        let target: XCUIElement? = ["target", "unhittable-target"].contains(name) ? XCUIElement() : nil
        if name == "unhittable-target" { target?.isHittable = false }
        let result = app.exercise(target: target)
        results[name] = ["result": result, "raw": app.raw, "drags": app.drags, "origins": app.origins,
                         "snapshots": app.snapshotCalls, "liveFrameReads": app.liveFrameReads,
                         "observations": app.observations, "failures": AssertionPort.failures]
    }
    return results
}
MainActor.assumeIsolated {
    let data = try! JSONSerialization.data(withJSONObject: runTraversalCases(), options: [.sortedKeys])
    print("TRAVERSAL_FIXTURE " + String(decoding: data, as: UTF8.self))
}
`;
  const root = mkdtempSync(resolve(tmpdir(), 'ms-r32-traversal-')); mkdirSync(resolve(root, 'cache'));
  try {
    const path = resolve(root, 'fixture.swift'); writeFileSync(path, fixture);
    const output = execFileSync('/usr/bin/swift', ['-module-cache-path', resolve(root, 'cache'), path], { encoding: 'utf8', timeout: 25000 });
    const line = output.split('\n').find(value => value.startsWith('TRAVERSAL_FIXTURE '));
    assert(line, 'Actual Swift traversal fixture must finish and emit its result before semantic assertions');
    traversalHostResults = JSON.parse(line.slice('TRAVERSAL_FIXTURE '.length));
    return traversalHostResults;
  } finally { rmSync(root, { recursive: true, force: true }); }
}
const gestureCount = result => result.raw.length + result.drags.length;

test('actual complete Swift stable traversal finishes both ends before causal bounded-gesture assertion', { timeout: 30000 }, () => {
  const stable = actualTraversalResults().stable;
  assert.deepEqual(stable.failures, [], 'Baseline actual traversal must complete without an assertion failure');
  assert.equal(stable.result, false); assert.equal(stable.snapshots, 10);
  assert.equal(stable.observations.length, 10); assert.equal(gestureCount(stable), 6);
  assert.equal(stable.raw.length, 0, 'Each existing iteration must use a bounded coordinate gesture instead of a raw list swipe');
  assert.equal(stable.drags.length, 6);
  assert.deepEqual(stable.drags.map(drag => [drag.start, drag.end]), [
    ...Array.from({ length: 3 }, () => [[140, 120], [140, 320]]),
    ...Array.from({ length: 3 }, () => [[140, 320], [140, 120]]),
  ]);
  assert(stable.drags.every(drag => drag.duration === 0.05));
  assert(stable.origins.length > 0 && stable.origins.every(origin => origin[0] === 0 && origin[1] === 0));
  assert.equal(stable.liveFrameReads, 0, 'Use the cached immutable root/clip, not live app/list frames');
});

test('actual traversal binds each clipped drag to the root from that same snapshot', () => {
  const result = actualTraversalResults()['snapshot-binding'];
  assert.deepEqual(result.failures, []); assert.equal(result.snapshots, 10);
  assert.equal(result.raw.length, 0); assert.equal(result.drags.length, 6); assert.equal(result.liveFrameReads, 0);
  result.drags.forEach((drag, index) => {
    const points = drag.snapshot % 2 === 0 ? [[160, 132.5], [160, 357.5]] : [[140, 120], [140, 320]];
    assert.deepEqual([drag.start, drag.end], index < 3 ? points : [...points].reverse());
    assert(drag.start.every(Number.isFinite) && drag.end.every(Number.isFinite));
  });
});

test('actual traversal keeps identity/clipping changes, elastic keys and three unchanged samples at both ends', () => {
  const results = actualTraversalResults();
  for (const name of ['identity-change', 'clipping-change']) {
    assert.deepEqual(results[name].failures, []); assert.equal(gestureCount(results[name]), 7);
    assert.equal(results[name].snapshots, 11);
  }
  assert(results['identity-change'].observations.flat().includes('lb-secret-name-one'));
  assert(results['identity-change'].observations.flat().includes('lb-secret-name-next'));
  assert.deepEqual(results.bounce.failures, []); assert.equal(gestureCount(results.bounce), 6);
});

test('actual traversal retains target, empty-list, unproven-end and duplicate assertion semantics', () => {
  const results = actualTraversalResults();
  assert.equal(results.target.result, true); assert.equal(results.target.snapshots, 1);
  assert.equal(gestureCount(results.target), 0); assert.deepEqual(results.target.failures, []);
  assert.equal(results['unhittable-target'].result, false); assert.equal(gestureCount(results['unhittable-target']), 6);
  assert.deepEqual(results.empty.failures, []); assert.equal(gestureCount(results.empty), 6);
  assert(results.empty.observations.every(rows => rows.length === 0));
  assert.equal(results['unproven-empty'].result, false); assert.equal(gestureCount(results['unproven-empty']), 128);
  assert(results['unproven-empty'].failures.some(value => value.includes('both independently proven list ends')));
  assert.equal(results.duplicate.result, false);
  assert(results.duplicate.failures.some(value => value.includes('Duplicate same-ID rows')));
  assert.equal(gestureCount(results.duplicate), 6, 'Nonthrowing XCTest assertion ports must not invent early abort semantics');
});

test('actual scoped snapshot refuses invalid ownership, finite geometry and traversal budgets without input', () => {
  const results = actualTraversalResults();
  for (const name of ['missing-window', 'wrong-window', 'ambiguous-route', 'ambiguous-list', 'missing-route', 'missing-list',
    'nonfinite-root', 'nonfinite-window', 'nonfinite-route', 'nonfinite-list', 'empty-root', 'empty-clip', 'node-budget']) {
    const result = results[name];
    assert.equal(result.result, false, name); assert.equal(gestureCount(result), 0, name);
    assert.equal(result.snapshots, 3, name);
    assert(result.failures.some(value => value.includes('Immutable list viewport unavailable')), name);
  }
});


import { runPassiveCardFixture } from './ios-scroll-passive-card-fixture.mjs';
let passiveCardHostResults;
const passiveCardResults = () => passiveCardHostResults ??= runPassiveCardFixture(source, appSource);

test('actual passive exact card remains reachable without a hittable container in two and three card lists', { timeout: 30000 }, () => {
  const results = passiveCardResults();
  for (const name of ['passive-two', 'passive-three']) {
    const result = results['traverse:' + name];
    assert.deepEqual(result.failures, [], name + ': original traversal must finish before reachability assertion');
    assert(result.snapshots >= 1 && result.observations >= 1, name + ': actual immutable traversal ran');
    assert(result.visibleChecks >= 1, name + ': original target visibility predicate ran');
    assert.equal(result.targetExists, true); assert.equal(result.targetHittable, false);
    assert.equal(result.actions, 0); assert.equal(result.raw, 0); assert.equal(result.liveFrameReads, 0);
    assert.equal(result.result, true, name + ': scoped exact passive card must not require a container hit point');
  }
});

test('actual passive exact card validates current scope in both fastpath and traversal before accepting', () => {
  const results = passiveCardResults();
  for (const mode of ['find', 'traverse']) {
    for (const name of ['passive-two', 'passive-three', 'partial-card']) {
      const result = results[mode + ':' + name];
      assert.equal(result.result, true, mode + ':' + name); assert.deepEqual(result.failures, []);
      assert(result.snapshots >= 1); assert.equal(result.actions, 0); assert.equal(result.raw, 0);
    }
    for (const name of ['wrong-id', 'wrong-name-id', 'missing-card', 'missing-name', 'duplicate-card', 'duplicate-name',
      'offclip-card', 'offclip-name', 'nonfinite-card', 'nonfinite-name', 'empty-card', 'empty-name',
      'missing-route', 'missing-list', 'duplicate-route', 'duplicate-list', 'missing-window', 'foreign-card', 'foreign-name',
      'equal-frame-foreign-list', 'equal-frame-foreign-route', 'nonfinite-root', 'nonfinite-window', 'nonfinite-route',
      'nonfinite-list', 'empty-clip', 'node-budget', 'snapshot-error']) {
      const result = results[mode + ':' + name];
      assert.equal(result.result, false, mode + ':' + name);
      assert.equal(result.actions, 0); assert.equal(result.raw, 0);
    }
  }
});

test('actual generic action targets retain hittability and discovery never dispatches an action', () => {
  const results = passiveCardResults();
  for (const mode of ['find', 'traverse']) {
    assert.equal(results[mode + ':action-unhittable'].result, false, mode + ': action requires hittability');
    assert.equal(results[mode + ':action-hittable'].result, true);
  }
  for (const result of Object.values(results)) assert.equal(result.actions, 0);
});

import { runBatchReadinessFixture } from './ios-batch-readiness-fixture.mjs';
test('actual CaseFour readiness completes two phases with one full sweep per phase', { timeout: 30000 }, () => {
  const caseSource = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseFourIosConcurrentRecoveryUITest.swift', import.meta.url), 'utf8');
  const result = runBatchReadinessFixture(source, appSource, caseSource);
  assert.deepEqual(result.failures, [], 'Actual old complete methods must finish successfully before the causal count');
  assert.deepEqual(result.mapsValid, [true, true]); assert.deepEqual(result.lastSelectedValid, [true, true]);
  assert.equal(result.unrelatedRetained, true); assert.equal(result.actions, 0); assert.equal(result.raw, 0);
  assert(result.waits.every(timeout => timeout > 0 && timeout <= 180), 'Original shared phase bound is not extended');
  assert(result.phaseSweeps.every((sweeps, index) => result.phaseDrags[index] === 6 * sweeps), 'Each counted sweep actually proves both stable ends');
  assert.equal(result.phaseSweeps.reduce((sum, count) => sum + count, 0), 2, 'Two readiness phases require two complete sweeps, not five');
  assert.deepEqual(result.phaseSweeps, [1, 1]); assert.deepEqual(result.phaseWaits, [1, 1]);
  assert.equal(result.passcodes, 2);
});

import { runBatchNegativeFixture } from './ios-batch-negative-fixture.mjs';
let batchNegativeHostResults;
function batchNegativeResults() {
  const caseSource = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseFourIosConcurrentRecoveryUITest.swift', import.meta.url), 'utf8');
  return batchNegativeHostResults ??= runBatchNegativeFixture(source, appSource, caseSource);
}

test('actual batch resolves all names only after both ends including cards observed on different pages', { timeout: 30000 }, () => {
  const results = batchNegativeResults();
  for (const [name, count] of [['two', 2], ['three', 3], ['disjoint', 3]]) {
    const result = results[name]; assert.equal(result.resolved, count, name); assert.equal(result.exactMap, true);
    assert.equal(result.failed, false); assert.equal(result.sweeps, 1); assert(result.snapshots >= 10);
    assert(result.drags >= 6); assert.equal(result.priorRetained, true); assert.equal(result.lastSelected, true);
    assert.equal(result.actions, 0); assert.equal(result.raw, 0);
    if (name !== 'disjoint') { assert.equal(result.snapshots, 10); assert.equal(result.drags, 6); }
  }
});

test('actual batch refuses late ambiguity and invalid metadata without partial selector publication', () => {
  const results = batchNegativeResults();
  const exceptions = new Set(['two', 'three', 'disjoint', 'empty-input', 'fresh-phase', 'shared-budget', 'empty-phase', 'setup:success', 'setup:main-failure', 'setup:final-failure']);
  for (const [name, result] of Object.entries(results)) {
    if (exceptions.has(name)) continue;
    assert.equal(result.resolved, 0, name); assert.equal(result.mapUnchanged, true, name); assert.equal(result.failed, true, name);
    assert.equal(result.actions, 0); assert.equal(result.raw, 0);
  }
  for (const name of ['duplicate-input', 'empty-input-name', 'zero-budget', 'large-budget', 'nan-budget']) {
    assert.equal(results[name].snapshots, 0); assert.deepEqual(results[name].secretsBudgets, []);
  }
  assert(results['late-duplicate'].snapshots >= 10); assert.equal(results['unproven-end'].drags, 128);
  assert(results['missing-expected'].sweeps > 1); assert.equal(results['missing-expected'].elapsed, 25);
});

test('actual batch shares phase budget and fresh proof while false readiness prevents creation or READY', () => {
  const results = batchNegativeResults();
  assert.equal(results['fresh-phase'].first, 3); assert.equal(results['fresh-phase'].second, 0);
  assert.equal(results['fresh-phase'].unchanged, true); assert.equal(results['fresh-phase'].failed, true);
  const shared = results['shared-budget']; assert.deepEqual(shared.waits, [180]);
  assert.equal(shared.secretsBudgets.length, 1); assert.equal(shared.requireBudgets.length, 1);
  assert(shared.secretsBudgets.every(value => value > 0 && value <= 5));
  assert(shared.requireBudgets.every(value => value > 0 && value <= 5));
  assert.equal(shared.elapsed, 180); assert.equal(shared.unchanged, true); assert.equal(shared.failed, true);
  for (const key of ['empty-input', 'empty-phase']) assert.equal(results[key].snapshots, 0);
  assert.equal(results['empty-input'].mapUnchanged, true); assert.equal(results['empty-input'].failed, false);
  assert.deepEqual(results['empty-phase'], { waits: 0, sweeps: 0, snapshots: 0, passcodes: 0, failed: false });
  assert.deepEqual(results['setup:success'], { creates: 1, ready: 1, failed: false, sweeps: 2 });
  assert.deepEqual(results['setup:main-failure'], { creates: 0, ready: 0, failed: true, sweeps: 0 });
  assert.deepEqual(results['setup:final-failure'], { creates: 1, ready: 0, failed: true, sweeps: 2 });
});
