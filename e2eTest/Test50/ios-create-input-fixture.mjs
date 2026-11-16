import assert from 'node:assert/strict';

// Complete owning extension; ports expose observations/events, never ACK decisions.
export function buildCreateInputFixture(source) {
  const prefix = 'import XCTest\n\n@MainActor\nextension XCUIApplication {\n';
  assert(source.startsWith(prefix));
  const end = source.lastIndexOf('\n}');
  assert(end > prefix.length);
  const methods = source.slice(prefix.length, end);
  assert(methods.includes('func lbReplaceField(') && methods.includes('private func lbCreateFieldState('));
  const generated = String.raw`import Foundation
import CoreGraphics
@MainActor enum Clock { static var now: TimeInterval = 0 }
@MainActor final class ProcessInfo {
    static let processInfo = ProcessInfo()
    var systemUptime: TimeInterval { Clock.now }
}
@MainActor enum AssertionPort { static var failures: [String] = [] }
@MainActor enum DiagnosticPort { static var rows: [String] = [] }
@MainActor func print(_ items: Any..., separator: String = " ", terminator: String = "\n") {
    DiagnosticPort.rows.append(items.map { String(describing: $0) }.joined(separator: separator))
}
@MainActor func XCTFail(_ message: String) { AssertionPort.failures.append(message) }
@MainActor func XCTAssertTrue(_ value: Bool, _ message: String = "") { if !value { XCTFail(message) } }
@MainActor enum HostElementType { case any, window, other, button, textView, keyboard }
@MainActor protocol XCUIElementSnapshot {
    var identifier: String { get }; var frame: CGRect { get }; var elementType: HostElementType { get }
    var isEnabled: Bool { get }; var value: Any? { get }; var children: [any XCUIElementSnapshot] { get }
}
@MainActor final class SnapshotNode: XCUIElementSnapshot {
    let identifier: String, frame: CGRect, elementType: HostElementType
    let isEnabled: Bool, rawValue: Any?
    let rawChildren: [any XCUIElementSnapshot]
    let onValue: (() -> Void)?, onChildren: (() -> Void)?
    var value: Any? { onValue?(); return rawValue }
    var children: [any XCUIElementSnapshot] { onChildren?(); return rawChildren }
    init(_ id: String, _ frame: CGRect, _ children: [any XCUIElementSnapshot] = [], type: HostElementType = .textView,
         enabled: Bool = true, value: Any? = nil, onValue: (() -> Void)? = nil, onChildren: (() -> Void)? = nil) {
        identifier = id; self.frame = frame; rawChildren = children; elementType = type
        isEnabled = enabled; rawValue = value; self.onValue = onValue; self.onChildren = onChildren
    }
}
@MainActor class XCUIElement {
    let identifier: String
    weak var owner: XCUIApplication?
    var rawExists = true, rawEnabled = true, hittable = true
    var exists: Bool {
        get { owner?.liveRead(); return rawExists }
        set { rawExists = newValue }
    }
    var isEnabled: Bool {
        get { owner?.liveRead(); return rawEnabled }
        set { rawEnabled = newValue }
    }
    var isHittable: Bool {
        get { owner?.liveRead(); if owner?.mode == "late-hittable" { Clock.now = 61 }; if owner?.mode == "within60-hittable" { Clock.now = 31 }; return hittable }
        set { hittable = newValue }
    }
    var rawType: HostElementType = .textView
    var elementType: HostElementType {
        get { owner?.liveRead(); return rawType }
        set { rawType = newValue }
    }
    var rawFrame = CGRect(x: 40, y: 180, width: 320, height: 48)
    var frame: CGRect {
        get { owner?.liveRead(); return rawFrame }
        set { rawFrame = newValue }
    }
    var stored = "", rawPlaceholder: String? = "Placeholder"
    var placeholderValue: String? {
        get { owner?.liveRead(); return rawPlaceholder }
        set { rawPlaceholder = newValue }
    }
    init(_ id: String = "", owner: XCUIApplication? = nil) { identifier = id; self.owner = owner }
    var value: Any? {
        owner?.liveRead(); owner?.flush(self)
        if let app = owner, app.snapshots > 0 {
            app.scalarReadsBySnapshot[app.snapshots, default: 0] += 1
            app.events.append("scalar:\(app.snapshots)")
        }
        var observed: Any? = stored
        if ["placeholder", "snap-nil-placeholder", "snap-nonstring-placeholder"].contains(owner?.mode ?? "") && stored.isEmpty { observed = placeholderValue }
        if let app = owner {
            if app.mode == "late-value" || (app.mode == "late-ack-value" && app.attempted > 0) { Clock.now = 61 }
            if app.snapshots > 0 {
                let ack = app.attempted > 0
                if app.mode == "live-nil-pre" || (app.mode == "live-nil-ack" && ack) { observed = nil }
                if app.mode == "live-other-pre" || (app.mode == "live-other-ack" && ack) { observed = ["CANARY_PRIVATE_VALUE": "CANARY_DYNAMIC_DESCRIPTION"] }
                if app.mode == "live-wrong-pre" || (app.mode == "live-wrong-ack" && ack) { observed = "CANARY_PRIVATE_TEXT" }
                if app.mode == "live-late-pre" || (app.mode == "live-late-ack" && ack) { Clock.now = 61 }
            }
            app.liveReturnClasses.append(observed == nil ? "nil" : (observed is String ? "string" : "other"))
            app.recordReturned(observed)
        }
        return observed
    }
    func tap() { owner?.tap(self, coordinate: false) }
    func typeText(_ text: String) { owner?.input(text, target: self, application: false) }
}
@MainActor struct HostQuery {
    let app: XCUIApplication, id: String?
    var elements: [XCUIElement] { app.query(id) }
    var count: Int { app.liveRead(); return elements.count }
    var firstMatch: XCUIElement { element(boundBy: 0) }
    func element(boundBy index: Int) -> XCUIElement {
        if elements.indices.contains(index) { return elements[index] }
        let missing = XCUIElement(id ?? "", owner: app); missing.exists = false; return missing
    }
    func matching(identifier: String) -> HostQuery { HostQuery(app: app, id: identifier) }
    subscript(_ identifier: String) -> XCUIElement { matching(identifier: identifier).firstMatch }
}
@MainActor final class HostCoordinate {
    let app: XCUIApplication, point: CGPoint
    init(_ app: XCUIApplication, _ point: CGPoint = .zero) { self.app = app; self.point = point }
    func withOffset(_ offset: CGVector) -> HostCoordinate { HostCoordinate(app, CGPoint(x: point.x + offset.dx, y: point.y + offset.dy)) }
    func tap() {
        app.coordinateTaps += 1
        if let field = app.fields.first(where: { $0.rawFrame.contains(point) }) { app.tap(field, coordinate: true) }
    }
    func press(forDuration: TimeInterval, thenDragTo: HostCoordinate) { app.drags += 1 }
}
enum XCUIKeyboardKey: String { case delete = "\u{8}" }
@MainActor final class XCUIApplication: XCUIElement {
    let mode: String, requested: String
    var fields: [XCUIElement] = []
    var modal: XCUIElement!, viewport: XCUIElement!, keyboard: XCUIElement!
    var focused: XCUIElement?, events: [String] = [], submitted: [String] = []
    var applicationCalls = 0, fieldCalls = 0, attempted = 0, deletions = 0, otherInputs = 0
    var taps = 0, coordinateTaps = 0, drags = 0, submits = 0, finds = 0, waitTimeouts: [Double] = []
    var liveReads = 0, snapshots = 0, childrenReads = 0, snapshotValueReads = 0, firstPollCharges = 0
    var ackWaiterBudgets: [Double] = [], reachBudgets: [Double] = []
    var inputCallSizes: [Int] = [], deleteCallSizes: [Int] = [], callExpected: String?
    var scalarReadsBySnapshot: [Int: Int] = [:], liveReturnClasses: [String] = [], snapshotValueClasses: [String] = []
    var pending: [String: String] = [:], pendingReads: [String: Int] = [:], expireAfterInput = false
    init(_ mode: String, _ requested: String, id: String = "lb-create-name") {
        self.mode = mode; self.requested = requested; super.init()
        owner = self
        frame = CGRect(x: 0, y: 0, width: 400, height: 800)
        modal = XCUIElement("lb-create", owner: self); modal.frame = CGRect(x: 20, y: 100, width: 360, height: 480)
        viewport = XCUIElement("lb-create-fields", owner: self); viewport.frame = CGRect(x: 30, y: 150, width: 340, height: 400)
        keyboard = XCUIElement("keyboard", owner: self); keyboard.frame = CGRect(x: 0, y: 600, width: 400, height: 200); keyboard.exists = false
        let target = XCUIElement(id, owner: self), other = XCUIElement("lb-create-value", owner: self)
        other.frame.origin.y = 300; fields = id == other.identifier ? [target] : [target, other]
        target.isHittable = mode != "coordinate"
        if ["old-text", "snap-nil-old", "snap-nonstring-old"].contains(mode) { target.stored = "Old🧩" }
        if mode == "clear-noop" { target.stored = "Old" }
        if mode == "duplicate" { fields.insert(XCUIElement(id, owner: self), at: 0) }
        if mode == "missing" { fields.removeAll { $0.identifier == id } }
        if mode == "disabled" { target.isEnabled = false }
        if mode == "wrong-type" { target.elementType = .button }
        if mode == "nonfinite" { target.frame.origin.x = .nan }
        if ["offclip", "snapshot-error", "node-budget", "tag-budget"].contains(mode) { target.frame.origin.x = -100 }
        if mode == "modal-disabled" { modal.isEnabled = false }
        if mode == "viewport-outside" { viewport.frame.origin.x = 500 }
        if mode == "keyboard-covered" { keyboard.exists = true; keyboard.frame.origin.y = 100 }
    }
    func descendants(matching: HostElementType) -> HostQuery { HostQuery(app: self, id: nil) }
    var keyboards: HostQuery { HostQuery(app: self, id: "keyboard") }
    func query(_ id: String?) -> [XCUIElement] {
        let all = [modal!, viewport!, keyboard!] + fields
        return all.filter { $0.rawExists && (id == nil || $0.identifier == id) }
    }
    func liveRead() { liveReads += 1; if ["native-ax-cost", "deferred-first-poll"].contains(mode) { Clock.now += 0.1 } }
    func tap(_ field: XCUIElement, coordinate: Bool) {
        taps += 1; events.append("tap"); focused = field; keyboard.exists = true
        if mode == "reflow" { field.frame.origin.y += 10 }
        if mode == "reflow-invalid" { field.isEnabled = false }
    }
    func input(_ text: String, target: XCUIElement?, application: Bool) {
        if application { applicationCalls += 1 } else { fieldCalls += 1 }
        if ["native-ax-cost", "deferred-first-poll"].contains(mode) { Clock.now += 0.4 }; if mode == "cumulative-within60" { Clock.now += 11 }; if mode == "cumulative-over60" { Clock.now += 13 }
        let destination = target ?? focused, incoming = Array(text)
        let deletes = incoming.filter { $0 == Character(XCUIKeyboardKey.delete.rawValue) }.count
        inputCallSizes.append(incoming.count - deletes); deleteCallSizes.append(deletes)
        let call = inputCallSizes.count; events.append("call:\(call):start")
        var expected = destination?.stored ?? ""
        for character in incoming {
            if character == Character(XCUIKeyboardKey.delete.rawValue) { if !expected.isEmpty { expected.removeLast() } }
            else { expected.append(character) }
        }
        callExpected = expected
        for (offset, character) in incoming.enumerated() {
            if character == Character(XCUIKeyboardKey.delete.rawValue) {
                deletions += 1; events.append("delete")
                if mode != "clear-noop", let destination, !destination.stored.isEmpty { destination.stored.removeLast() }
                continue
            }
            attempted += 1; submitted.append(String(character)); events.append("input:\(attempted):\(application ? "app" : "field")")
            if destination?.identifier != fields.first?.identifier { otherInputs += 1 }
            if mode == "drop-first-five" && attempted <= 5 { continue }
            if mode == "drop-second" && attempted == 2 { continue }
            if (mode == "drop-first-one" && attempted == 1) || (mode == "drop-third" && attempted == 3) || (mode == "drop-fifth" && attempted == 5) { continue }
            let recipient = mode == "wrong-recipient" ? fields.last : destination
            let sent = mode == "reorder-first-chunk" && call == 1 ? incoming[incoming.count - 1 - offset] : character
            let delivered = mode == "transform-second" && attempted == 2 ? "?" : String(sent)
            if mode == "pending-render" {
                pending[recipient!.identifier, default: recipient!.stored] += delivered
                pendingReads[recipient!.identifier] = 0
            } else { recipient?.stored += delivered }
            if mode == "duplicate-second" && attempted == 2 { recipient?.stored += delivered }
            if mode == "rollback-second" && attempted == 2 { recipient?.stored = "" }
            if mode == "replace-after-input" { destination?.exists = false }
            if mode == "scope-after-input" { destination?.isEnabled = false }
            if mode == "deadline-after-first" { Clock.now = 61 }
        }
        events.append("call:\(call):end")
    }
    override func typeText(_ text: String) { input(text, target: nil, application: true) }
    func flush(_ field: XCUIElement) {
        if let queued = pending[field.identifier] {
            pendingReads[field.identifier, default: 0] += 1
            if pendingReads[field.identifier]! >= 3 { field.stored = queued; pending.removeValue(forKey: field.identifier) }
        }
    }
    func record(_ observed: String) {
        let prefix = String(requested.prefix(attempted))
        events.append("read:\(attempted):\(observed == prefix ? "exact" : "other")")
        if let expected = callExpected { events.append("call-ack:\(inputCallSizes.count):\(observed == expected ? "exact" : "other")") }
    }
    func read(_ field: XCUIElement) { flush(field); record(field.stored) }
    func recordReturned(_ value: Any?) {
        if let text = value as? String { record(text) }
        else { events.append("read:\(attempted):\(value == nil ? "nil" : "other")") }
    }
    func coordinate(withNormalizedOffset: CGVector) -> HostCoordinate { HostCoordinate(self) }
    func snapshot() throws -> any XCUIElementSnapshot {
        snapshots += 1; events.append("scope:\(snapshots)")
        if ["native-ax-cost", "deferred-first-poll"].contains(mode) { Clock.now += 0.1 }
        defer {
            if mode == "snap-late" || (mode == "late-ack-value" && attempted > 0) { Clock.now = 61 }
            let final = attempted == requested.count && attempted > 0 && snapshots == fieldCalls * 2 + 1
            if mode == "within60-final" && final { Clock.now = 31 }
            if mode == "exact60-final" && final { Clock.now = 60 }
            if mode == "past60-final" && final { Clock.now = 61 }
            if mode == "within60-ack" && attempted == requested.count && snapshots == fieldCalls * 2 { Clock.now = 31 }
        }
        if ["snapshot-error", "snap-error"].contains(mode) { throw HostError.snapshot }
        var nodes: [any XCUIElementSnapshot] = []
        for field in fields where field.rawExists {
            flush(field)
            let value = mode == "placeholder" && field.stored.isEmpty ? field.rawPlaceholder! : field.stored
            var projected: Any? = mode.hasPrefix("snap-nonstring") ? (42 as Any) : (value as Any)
            if mode.hasPrefix("snap-nil-") { projected = nil }
            if mode == "snap-canary-other" { projected = ["CANARY_PRIVATE_VALUE": "CANARY_DYNAMIC_DESCRIPTION"] }
            if mode == "snap-canary-string" { projected = "CANARY_PRIVATE_TEXT" }
            snapshotValueClasses.append(projected == nil ? "nil" : (projected is String ? "string" : "other"))
            var rectangle = mode == "snap-offclip" ? CGRect(x: 800, y: 800, width: 10, height: 10) : field.rawFrame
            if mode == "snap-nonfinite-field" { rectangle.origin.x = .nan }
            let node = SnapshotNode(field.identifier, rectangle, type: mode == "snap-wrong-type" ? .button : field.rawType,
                enabled: mode != "snap-disabled" && field.rawEnabled, value: projected, onValue: { self.snapshotValueReads += 1; self.recordReturned(projected) })
            if mode != "snap-missing-field" || field.identifier != fields.first?.identifier { nodes.append(node) }
            if mode == "snap-duplicate-field" && field.identifier == fields.first?.identifier { nodes.append(node) }
        }
        var modalFrame = modal.rawFrame, viewportFrame = viewport.rawFrame
        if mode == "snap-modal-outside" { modalFrame.origin.x = 500 }
        if mode == "snap-viewport-outside" { viewportFrame.origin.x = 500 }
        let modalNode = SnapshotNode(modal.identifier, modalFrame, type: .other, enabled: modal.rawEnabled)
        let viewportNode = SnapshotNode(viewport.identifier, viewportFrame, type: .other, enabled: viewport.rawEnabled)
        if mode != "snap-missing-modal" { nodes.append(modalNode) }
        if mode == "snap-duplicate-modal" { nodes.append(modalNode) }
        if mode != "snap-missing-viewport" { nodes.append(viewportNode) }
        if mode == "snap-duplicate-viewport" { nodes.append(viewportNode) }
        if keyboard.rawExists {
            var rectangle = mode == "snap-keyboard-covered" ? modal.rawFrame : keyboard.rawFrame
            if mode == "snap-nonfinite-keyboard" { rectangle.origin.x = .nan }
            nodes.append(SnapshotNode("", rectangle, type: .keyboard))
        }
        if mode == "node-budget" { nodes += (0..<8193).map { _ in SnapshotNode("", rawFrame) } }
        if mode == "tag-budget" { nodes += (0..<65).map { SnapshotNode("lb-create-extra-\($0)", rawFrame) } }
        if mode == "snap-wide-frontier" {
            return SnapshotNode("", rawFrame, (0..<20000).map { _ in SnapshotNode("", rawFrame) }, type: .other,
                onChildren: { self.childrenReads += 1 })
        }
        if mode == "snap-no-window" { return SnapshotNode("", rawFrame, nodes, type: .other) }
        var foreign: [any XCUIElementSnapshot] = []
        if mode == "snap-foreign-field" || mode == "snap-foreign-viewport" {
            let id = mode == "snap-foreign-field" ? fields[0].identifier : viewport.identifier
            foreign = nodes.filter { $0.identifier == id }; nodes.removeAll { $0.identifier == id }
        }
        var windowFrame = rawFrame, rootFrame = rawFrame
        if mode == "snap-window-nonfinite" { windowFrame.origin.x = .nan }
        if mode == "snap-root-nonfinite" { rootFrame.origin.x = .nan }
        var windows: [any XCUIElementSnapshot] = [SnapshotNode("", windowFrame, nodes, type: .window)]
        if !foreign.isEmpty { windows.append(SnapshotNode("", rawFrame, foreign, type: .window)) }
        return SnapshotNode("", rootFrame, windows, type: .other)
    }
    func lbWait(_ predicate: @escaping () -> Bool, timeout: TimeInterval = 180, message: String = "") {
        waitTimeouts.append(timeout)
        if message == "One active Create editor viewport is required" { reachBudgets.append(timeout) }
        if message == "Exact Create prefix acknowledgement was not observed" { ackWaiterBudgets.append(timeout) }
        let deadline = Clock.now + max(0, timeout)
        if mode == "deferred-first-poll" {
            firstPollCharges += 1
            guard Clock.now + 1 < deadline else { Clock.now = deadline; XCTFail(message); return }
            Clock.now += 1
        }
        for _ in 0...1000 {
            if predicate() { return }
            if Clock.now >= deadline { break }
            Clock.now += min(0.1, deadline - Clock.now)
        }
        XCTFail(message)
    }
    func lbDismissKeyboard() { keyboard.exists = false }
    func lbTap(_ id: String) { if id == "lb-create-submit" { submits += 1; modal.exists = false } }
    func lbConfirmSystemIfPresent() -> Bool { false }
    func lbHome() {}
    func lbFindSecret(_ name: String) -> String { finds += 1; return "fixture-id" }
}
enum HostError: Error { case snapshot }
@MainActor extension XCUIApplication {
${methods}
}
MainActor.assumeIsolated {
    let cases: [(String, String, String)] = [
        ("plain", "fixture-alpha", "lb-create-name"), ("coordinate", "fixture-alpha", "lb-create-name"),
        ("within60-hittable", "fixture-alpha", "lb-create-name"), ("within60-final", "fixture-alpha", "lb-create-name"),
        ("within60-ack", "1234567890123456789", "lb-create-value"), ("exact60-final", "fixture-alpha", "lb-create-name"),
        ("past60-final", "fixture-alpha", "lb-create-name"), ("cumulative-within60", "123456789012345", "lb-create-name"),
        ("cumulative-over60", "123456789012345", "lb-create-name"),
        ("old-text", "fixture-alpha", "lb-create-name"), ("placeholder", "fixture-alpha", "lb-create-name"),
        ("unicode", "é👩🏽‍💻e\u{301}", "lb-create-name"), ("value", "line1\nline2", "lb-create-value"),
        ("word", "puzzle", "lb-create-word-24"), ("empty", "", "lb-create-name"),
        ("literal-placeholder", "Placeholder", "lb-create-name"), ("clear-noop", "fixture-alpha", "lb-create-name"),
        ("late-hittable", "fixture-alpha", "lb-create-name"), ("late-value", "fixture-alpha", "lb-create-name"),
        ("late-ack-value", "fixture-alpha", "lb-create-name"), ("rollback-second", "fixture-alpha", "lb-create-name"),
        ("replace-after-input", "fixture-alpha", "lb-create-name"),
        ("drop-first-one", "fixture-alpha", "lb-create-name"), ("drop-third", "fixture-alpha", "lb-create-name"),
        ("drop-fifth", "fixture-alpha", "lb-create-name"), ("reorder-first-chunk", "fixture-alpha", "lb-create-name"),
        ("unicode-four", "é👩🏽‍💻e\u{301}X", "lb-create-name"), ("unicode-five", "é👩🏽‍💻e\u{301}XY", "lb-create-name"),
        ("value-nineteen", "1234567890123456789", "lb-create-value"),
        ("native-ax-cost", "fixture-alpha", "lb-create-name"), ("deferred-first-poll", "fixture-alpha", "lb-create-name"),
        ("drop-first-five", "fixture-alpha", "lb-create-name"), ("drop-second", "fixture-alpha", "lb-create-name"),
        ("transform-second", "fixture-alpha", "lb-create-name"), ("duplicate-second", "fixture-alpha", "lb-create-name"),
        ("wrong-recipient", "fixture-alpha", "lb-create-name"), ("scope-after-input", "fixture-alpha", "lb-create-name"),
        ("deadline-after-first", "fixture-alpha", "lb-create-name"), ("pending-render", "fixture-alpha", "lb-create-name"),
        ("reflow", "fixture-alpha", "lb-create-name"), ("reflow-invalid", "fixture-alpha", "lb-create-name"),
        ("missing", "fixture-alpha", "lb-create-name"), ("duplicate", "fixture-alpha", "lb-create-name"),
        ("disabled", "fixture-alpha", "lb-create-name"), ("wrong-type", "fixture-alpha", "lb-create-name"),
        ("nonfinite", "fixture-alpha", "lb-create-name"), ("offclip", "fixture-alpha", "lb-create-name"),
        ("modal-disabled", "fixture-alpha", "lb-create-name"), ("viewport-outside", "fixture-alpha", "lb-create-name"),
        ("keyboard-covered", "fixture-alpha", "lb-create-name"), ("snapshot-error", "fixture-alpha", "lb-create-name"),
        ("node-budget", "fixture-alpha", "lb-create-name"), ("tag-budget", "fixture-alpha", "lb-create-name")]
        + ["snap-error", "snap-wide-frontier", "snap-nonstring", "snap-offclip", "snap-wrong-type", "snap-disabled",
            "snap-missing-field", "snap-duplicate-field", "snap-missing-modal", "snap-duplicate-modal",
            "snap-missing-viewport", "snap-duplicate-viewport", "snap-no-window", "snap-foreign-field",
            "snap-foreign-viewport", "snap-keyboard-covered", "snap-late", "snap-nonfinite-field", "snap-nonfinite-keyboard",
            "snap-modal-outside", "snap-viewport-outside", "snap-root-nonfinite", "snap-window-nonfinite",
            "snap-nil-value", "snap-canary-other", "snap-canary-string",
            "snap-nil-placeholder", "snap-nil-old", "snap-nonstring-placeholder", "snap-nonstring-old",
            "live-nil-pre", "live-other-pre", "live-wrong-pre", "live-late-pre",
            "live-nil-ack", "live-other-ack", "live-wrong-ack", "live-late-ack"]
            .map { ($0, "fixture-alpha", "lb-create-name") }
    var rows: [[String: Any]] = []
    for (mode, requested, identifier) in cases {
        Clock.now = 0; AssertionPort.failures = []; DiagnosticPort.rows = []
        let app = XCUIApplication(mode, requested, id: identifier)
        Clock.now = 0; app.liveReads = 0
        let result: Any = app.lbReplaceField(identifier, value: requested)
        let target = app.fields.first(where: { $0.identifier == identifier })
        rows.append(["mode": mode, "failures": AssertionPort.failures.count, "attempted": app.attempted,
            "requested": requested.count, "inputSequenceExact": app.submitted.joined() == requested,
            "inputPrefixExact": app.submitted.joined() == String(requested.prefix(app.attempted)),
            "observedCharacters": target?.stored.count ?? -1, "finalExact": target?.stored == requested,
            "fieldCalls": app.fieldCalls, "applicationCalls": app.applicationCalls, "deletions": app.deletions,
            "inputCallSizes": app.inputCallSizes, "deleteCallSizes": app.deleteCallSizes,
            "taps": app.taps, "coordinateTaps": app.coordinateTaps, "events": app.events,
            "otherInputs": app.otherInputs, "submits": app.submits, "elapsed": Clock.now,
            "liveReads": app.liveReads, "snapshots": app.snapshots, "childrenReads": app.childrenReads,
            "diagnostics": DiagnosticPort.rows, "snapshotValueReads": app.snapshotValueReads,
            "firstPollCharges": app.firstPollCharges, "ackWaiterBudgets": app.ackWaiterBudgets, "reachBudgets": app.reachBudgets,
            "scalarReadsBySnapshot": app.scalarReadsBySnapshot.sorted { $0.key < $1.key }.map { ["snapshot": $0.key, "reads": $0.value] },
            "liveReturnClasses": app.liveReturnClasses, "snapshotValueClasses": app.snapshotValueClasses,
            "returnedBool": (result as? Bool) as Any? ?? NSNull()])
    }
    let data = try! JSONSerialization.data(withJSONObject: rows, options: [.sortedKeys])
    Swift.print(String(data: data, encoding: .utf8)!)
}
`;
  const owningStart = generated.indexOf('@MainActor extension XCUIApplication {');
  const mainStart = generated.indexOf('MainActor.assumeIsolated {');
  assert(owningStart > 0 && mainStart > owningStart);
  return {
    ports: generated.slice(0, owningStart),
    owning: 'import Foundation\nimport CoreGraphics\n' + generated.slice(owningStart, mainStart),
    main: 'import Foundation\n' + generated.slice(mainStart),
  };
}
