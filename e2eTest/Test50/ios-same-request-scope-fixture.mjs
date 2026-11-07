// Closed immutable AX trees and drivers only; no candidate window/ancestry algorithm lives here.
export const scopePorts = String.raw`
protocol XCUIElementSnapshot {
    var identifier: String { get }
    var label: String { get }
    var frame: CGRect { get }
    var elementType: HostElementType { get }
    var children: [any XCUIElementSnapshot] { get }
}
final class ScopeNode: XCUIElementSnapshot {
    let identifier: String, label: String
    let frame: CGRect
    let elementType: HostElementType
    let children: [any XCUIElementSnapshot]
    init(_ id: String, _ frame: CGRect, type: HostElementType = .other,
         children: [any XCUIElementSnapshot] = [], label: String = "") {
        identifier = id; self.frame = frame; elementType = type; self.children = children; self.label = label
    }
}
@MainActor enum ScopeOutput {
    static var lines: [String] = [], attempts = 0, dropped = false, drop = false
    static func reset() { lines = []; attempts = 0; dropped = false; drop = false }
}
@MainActor func print(_ items: Any..., separator: String = " ", terminator: String = "\n") {
    let line = items.map { String(describing: $0) }.joined(separator: separator)
    guard line.hasPrefix("LB_SHOW_SCOPE_V1 ") else { Swift.print(line, terminator: terminator); return }
    ScopeOutput.attempts += 1
    if ScopeOutput.drop { ScopeOutput.dropped = true; return }
    ScopeOutput.lines.append(line); Swift.print(line, terminator: terminator)
}
@MainActor func controlledScopeSnapshot(_ app: XCUIApplication) throws -> any XCUIElementSnapshot {
    app.snapshotCalls += 1
    let mode = app.scopeMode, bounds = CGRect(x: 0, y: 0, width: 300, height: 400)
    if mode == "snapshot-error" {
        throw NSError(domain: "SNAPSHOT_ERROR_CANARY", code: 1, userInfo: nil)
    }
    func copy(_ element: XCUIElement) -> ScopeNode {
        var rectangle = element.frame
        if (mode == "nonfinite-route" && element.identifier == "lb-secrets") ||
            (mode == "nonfinite-list" && element.identifier == "lb-secrets-list") { rectangle.origin.x = .nan }
        if mode == "empty-clip" && element.identifier == "lb-secrets-list" { rectangle.origin.x = 1000 }
        let label = mode == "association-snapshot-name" && element === app.nameNode ? "RENAMED_SNAPSHOT" : element.label
        let type = mode == "association-snapshot-type" && element === app.primary ? HostElementType.staticText : element.elementType
        return ScopeNode(element.identifier, rectangle, type: type,
            children: element.children.filter { $0.exists }.map(copy), label: label)
    }
    var children: [any XCUIElementSnapshot] = app.children.filter { $0.exists }.map(copy)
    if mode == "missing-snapshot-route" { children = [copy(app.list)] }
    if mode == "missing-snapshot-list" { children = [ScopeNode("lb-secrets", bounds)] }
    if mode == "duplicate-snapshot-route" { children.append(ScopeNode("lb-secrets", bounds)) }
    if mode == "duplicate-snapshot-list" { children.append(ScopeNode("lb-secrets-list", bounds)) }
    if mode == "node-budget" { children += (0..<8193).map { _ in ScopeNode("noise", bounds) } }
    if mode == "missing-window" { return ScopeNode("application", bounds, children: children) }
    if mode == "equal-window-rectangles" || mode == "foreign-window" {
        let second = mode == "foreign-window" ? bounds.offsetBy(dx: 10, dy: 10) : bounds
        // Separate physical snapshot nodes, even when their CGRect values are identical.
        let route = ScopeNode("lb-secrets", bounds)
        return ScopeNode("application", bounds, children: [
            ScopeNode("first-window", bounds, type: .window, children: [route]),
            ScopeNode("second-window", second, type: .window, children: [copy(app.list)])])
    }
    if mode.hasPrefix("metadata-") {
        guard let target = mode.hasSuffix("-card") ? app.card : mode.hasSuffix("-name") ? app.nameNode : app.primary else {
            preconditionFailure("Closed metadata fixture target is unavailable")
        }
        if mode.hasPrefix("metadata-missing-") { children.removeAll { $0.identifier == target.identifier } }
        if mode.hasPrefix("metadata-duplicate-") { children.append(copy(target)) }
        if mode.hasPrefix("metadata-foreign-") || mode.hasPrefix("metadata-unwindowed-") {
            children.removeAll { $0.identifier == target.identifier }
            let first = ScopeNode("window", bounds, type: .window, children: children)
            let other: any XCUIElementSnapshot = mode.hasPrefix("metadata-foreign-")
                ? ScopeNode("other-window", bounds, type: .window, children: [copy(target)]) : copy(target)
            return ScopeNode("application", bounds, children: [first, other])
        }
    }
    var rootFrame = bounds, windowFrame = bounds
    if mode == "nonfinite-root" { rootFrame.origin.x = .nan }
    if mode == "nonfinite-window" { windowFrame.origin.x = .nan }
    return ScopeNode("application", rootFrame, children: [
        ScopeNode("window", windowFrame, type: .window, children: children)])
}
@MainActor extension XCUIApplication {
    func snapshot() throws -> any XCUIElementSnapshot { try controlledScopeSnapshot(self) }
}
`;

export const scopeCases = String.raw`
@MainActor func runScopeCases() -> [String: Any] {
    var output: [String: Any] = [:]
    let modes = ["sibling", "equal-window-rectangles", "foreign-window", "missing-window",
        "missing-snapshot-route", "duplicate-snapshot-route", "missing-snapshot-list", "duplicate-snapshot-list",
        "nonfinite-root", "nonfinite-window", "nonfinite-route", "nonfinite-list", "empty-clip",
        "node-budget", "snapshot-error", "no-snapshot", "all-relations", "logger-drop",
        "after-ready-window", "after-ready-clip", "metadata-flat-all",
        "metadata-missing-card", "metadata-missing-name", "metadata-missing-primary",
        "metadata-duplicate-card", "metadata-duplicate-name", "metadata-duplicate-primary",
        "metadata-foreign-card", "metadata-foreign-name", "metadata-foreign-primary",
        "metadata-unwindowed-card", "metadata-unwindowed-name", "metadata-unwindowed-primary",
        "association-foreign-card", "association-foreign-name", "association-foreign-primary",
        "association-incomplete-owner", "association-role-alias", "association-name-tag-not-owner",
        "association-snapshot-name", "association-snapshot-type", "association-after-ready-owner",
        "association-after-ready-snapshot-name", "association-after-ready-snapshot-type"]
    for mode in modes {
        AssertionPort.reset()
        ProcessInfo.processInfo.environment = ["METASECRET_E2E_IDENTITY_NAMESPACE": "f2-00000000-0000-4000-8000-000000000001"]
        let app = XCUIApplication(), owner = ActualCaseFour(XCUIApplication())
        owner.replaceApp(app); owner.driveRequest(XCUIApplication.secretName, 1)
        let issued = owner.hasToken(); app.approveOriginalRequest()
        app.scopeMode = mode
        if mode == "sibling" || mode == "all-relations" {
            app.route.children = []; app.children.append(app.list)
        }
        if mode == "all-relations" || mode == "logger-drop" {
            app.card.children.removeAll { $0 === app.nameNode || $0 === app.primary }
            app.list.children += [app.nameNode, app.primary]
        }
        if mode.hasPrefix("metadata-") {
            app.route.children.removeAll { $0 === app.list }
            app.list.children.removeAll { $0 === app.card || $0 === app.nameNode || $0 === app.primary }
            app.card.children.removeAll { $0 === app.nameNode || $0 === app.primary }
            app.children += [app.list, app.card, app.nameNode, app.primary]
        }
        if mode == "no-snapshot" { app.primary.elementType = .staticText }
        if mode == "logger-drop" { ScopeOutput.drop = true; app.scopeMode = "equal-window-rectangles" }
        func foreignOwner(_ target: XCUIElement, complete: Bool = true) {
            let foreign = XCUIElement("lb-secret-FOREIGN_FULL_OPAQUE_ID")
            if complete {
                let control = XCUIElement("lb-primary-FOREIGN_FULL_OPAQUE_ID"); control.elementType = .button
                foreign.children = [XCUIElement("lb-secret-name-FOREIGN_FULL_OPAQUE_ID", label: "FOREIGN_NAME"), control]
            }
            app.list.children.removeAll { $0 === target }
            app.card.children.removeAll { $0 === target }
            app.children.removeAll { $0 === target }
            foreign.children.append(target); app.list.children.append(foreign)
        }
        if mode == "association-foreign-card" { foreignOwner(app.card) }
        if mode == "association-foreign-name" { foreignOwner(app.nameNode) }
        if mode == "association-foreign-primary" { foreignOwner(app.primary) }
        if mode == "association-incomplete-owner" { foreignOwner(app.primary, complete: false) }
        if mode == "association-role-alias" {
            let otherId = "name-" + XCUIApplication.passId
            let control = XCUIElement("lb-primary-" + otherId); control.elementType = .button
            app.nameNode.children = [XCUIElement("lb-secret-name-" + otherId, label: "FOREIGN_NAME"), control]
            app.card.children.removeAll { $0 === app.primary }; app.nameNode.children.append(app.primary)
        }
        if mode == "association-name-tag-not-owner" {
            app.card.children.removeAll { $0 === app.primary }; app.nameNode.children.append(app.primary)
        }
        if mode == "association-after-ready-owner" { app.afterReady = { foreignOwner(app.primary) } }
        if mode == "association-after-ready-snapshot-name" { app.afterReady = { app.scopeMode = "association-snapshot-name" } }
        if mode == "association-after-ready-snapshot-type" { app.afterReady = { app.scopeMode = "association-snapshot-type" } }
        if mode == "after-ready-window" { app.afterReady = { app.scopeMode = "equal-window-rectangles" } }
        if mode == "after-ready-clip" { app.afterReady = { app.scopeMode = "empty-clip" } }
        var probe = false
        if mode == "sibling" || mode == "equal-window-rectangles" {
            // The genuine old shared helper already exists; never require a future private API for RED.
            do { _ = try app.lbSecretsViewportSnapshot(); probe = true } catch { XCTFail("Controlled public snapshot setup failed") }
        }
        owner.driveShow(XCUIApplication.secretName, 1)
        output["scope-" + mode] = ["failures": AssertionPort.failures, "probe": probe, "issued": issued,
            "lookups": app.lookups, "exactReaches": app.exactReaches, "token": owner.hasToken(),
            "recoverAttempts": app.recoverAttempts, "showAttempts": app.showAttempts, "tapAttempts": app.tapAttempts,
            "recovers": app.recovers, "shows": app.shows, "pins": app.pinCalls, "valueRequires": app.valueRequires,
            "snapshotCalls": app.snapshotCalls, "diagnostics": ScopeOutput.lines,
            "diagnosticAttempts": ScopeOutput.attempts, "diagnosticDropped": ScopeOutput.dropped,
            "inputsMatched": app.inputsMatched]
    }
    return output
}
`;
