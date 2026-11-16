import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';

export function buildSingleFifoFixture(inbox, caller, observation) {
  const types = inbox.slice(inbox.indexOf('struct LiveboardIncomingExpected {'), inbox.indexOf('\n@MainActor'));
  const methods = inbox.slice(inbox.indexOf('    func lbLoadFifo('), inbox.lastIndexOf('\n}'));
  const validatorStart = observation.indexOf('func liveboardRecoveryRequestIdIsValid(');
  const validator = observation.slice(validatorStart, observation.indexOf('\n}\n', validatorStart) + 3);
  const branchStart = caller.indexOf('            if cycle.offlineReceiver == "ios" {');
  const sender = '            if let ownSecret = cycle.senderSecrets["ios"] {';
  const branchEnd = caller.indexOf(sender, caller.indexOf(sender, branchStart) + 1);
  const receiver = caller.slice(branchStart, branchEnd);
  const legacyWait = caller.slice(caller.indexOf('    private func waitForIncomingRecovery('), caller.indexOf('    private func waitForSecretAfterJoin('));
  const legacyApprove = caller.slice(caller.indexOf('    private func approveIncomingRecovery('), caller.indexOf('    private func skipOnboardingIfNeeded('));
  assert(types.includes('struct LiveboardFifoContext: Decodable'));
  assert(methods.startsWith('    func lbLoadFifo(') && methods.includes('func lbFifoStep('));
  assert(receiver.includes('var remaining = originalSteps') && receiver.includes('app.lbDecideCurrent(approve: true)'));
  assert(validator.startsWith('func liveboardRecoveryRequestIdIsValid('));
  const cardinality = methods.slice(0, methods.indexOf('{')).includes('expectedRequestCount:') ? ', expectedRequestCount: count' : '';
  return String.raw`import Foundation
import CoreGraphics
${types}
${validator}
struct LiveboardIncomingRevealState { var gestures = 0 }
struct LiveboardIncomingSenderSample {}
@MainActor enum Ports {
    static var data: Data?, failures: [String] = [], lines: [String] = [], events: [String] = []
    static var loads = 0, waits: [Double] = [], grants = 0, permitAllowed = false
    static var permits: [String] = [], expectedPlain: String?
    static let scope = ["runId": "controlled-run", "testNumber": "6", "blockId": "controlled-block"]
    static func reset(_ value: [String: Any]?) {
        data = value.map { try! JSONSerialization.data(withJSONObject: $0) }
        failures = []; lines = []; events = []; loads = 0; waits = []; grants = 0; permitAllowed = false
        permits = []; expectedPlain = nil
    }
}
@MainActor func XCTFail(_ message: String) { Ports.failures.append(message) }
@MainActor func print(_ message: String) { Ports.lines.append(message); Ports.events.append(message.contains("INCOMING_VISIBLE") ? "visible" : "emit") }
@MainActor func liveboardCoordinatorData(_ url: URL) -> Data? { Ports.loads += 1; return Ports.data }
@MainActor final class XCUIApplication {
    var heads: [LiveboardIncoming] = [], index = 0, readable = true
    var reads = 0, expansions = 0, attempts = 0, decisions = 0, homes = 0, recover = 0, closes = 0
    var current: LiveboardIncoming { heads[min(index, heads.count - 1)] }
    func lbWait(_ predicate: () -> Bool, timeout: Double = 30) {
        Ports.waits.append(timeout)
        for _ in 0..<3 { if predicate() { return } }
        XCTFail("Closed bounded predicate did not complete")
    }
    func lbHome() { homes += 1 }
    func lbRevealIncomingInbox(_ state: inout LiveboardIncomingRevealState) -> CGRect? {
        state.gestures += 1; return CGRect(x: 0, y: 0, width: 100, height: 100)
    }
    func lbReadIncoming(observed: ((LiveboardIncomingSenderSample) -> Void)? = nil) -> LiveboardIncoming? {
        reads += 1; return readable ? current : nil
    }
    func lbIncomingSenderContained(in clip: CGRect) -> Bool { true }
    func lbRecordIncomingSenderObservation(_ sample: LiveboardIncomingSenderSample) {}
    @discardableResult func lbIncoming(_ expected: LiveboardIncomingExpected, timeout: Double = 180) -> LiveboardIncoming {
        expansions += 1; return current
    }
    func lbDecideCurrent(approve: Bool) {
        attempts += 1; Ports.events.append("attempt")
        guard approve && Ports.permitAllowed else { return }
        decisions += 1; index += 1; Ports.permitAllowed = false; Ports.events.append("decision")
    }
${methods}
}
struct Approval { let platform: String; let secret: String }
struct Cycle { let number: Int; let senderSecrets: [String: String]; let approvals: [Approval]; var offlineReceiver: String? = nil }
struct Coordinator { let fifoScope: [String: String] }
@MainActor final class ReceiverHarness {
    let app: XCUIApplication
    var coordinator: Coordinator? = Coordinator(fifoScope: Ports.scope)
    init(_ app: XCUIApplication) { self.app = app }
    func waitForApproval(platform: String, cycle: Int) {
        Ports.events.append("permit"); Ports.permits.append(platform)
        if platform == "ios-offline-stop" { Ports.grants += 1; return }
        let visible = Ports.lines.last(where: { $0.contains("INCOMING_VISIBLE") })
        let suffix = "claimId=" + app.current.requestId
        Ports.permitAllowed = Ports.expectedPlain.map { visible == $0 } ?? (visible?.hasSuffix(suffix) == true)
        if Ports.permitAllowed { Ports.grants += 1 }
    }
    func requestRecovery(_ name: String, cycle: Int) { app.recover += 1 }
    func closeShowSecretDialog() { app.closes += 1 }
${legacyWait}
${legacyApprove}
    func run(_ cycle: Cycle) {
${receiver}
    }
}
let ids = ["11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222", "33333333-3333-4333-8333-333333333333"]
func value(_ count: Int) -> [String: Any] {
    let names = ["android", "web", "ios"]
    let identities = Dictionary(uniqueKeysWithValues: names.map { ($0, ["deviceId": "device-" + $0, "deviceName": "test-" + $0]) })
    let requests: [[String: Any]] = (0..<count).map { n in
        let platform = names[n]
        return ["kind": "recover", "requestId": ids[n], "passId": "pass-\(n)", "requesterId": "device-" + platform,
                "requesterName": "test-" + platform, "subject": "subject-\(n)",
                "eligiblePeerIds": names.filter { $0 != platform }.map { "device-" + $0 }]
    }
    let approvals: [[String: Any]] = (0..<count).map { n in
        ["logicalStep": n + 1, "approverPlatform": "ios", "passId": "pass-\(n)", "subject": "subject-\(n)", "action": "approve"]
    }
    return ["version": 1, "runId": "controlled-run", "testNumber": 6, "blockId": "controlled-block", "cycle": 1,
            "identities": identities, "requests": requests, "approvals": approvals]
}
func changeRequest(_ input: [String: Any], _ index: Int, _ key: String, _ replacement: Any) -> [String: Any] {
    var result = input, requests = input["requests"] as! [[String: Any]]
    requests[index][key] = replacement; result["requests"] = requests; return result
}
func changeApproval(_ input: [String: Any], _ key: String, _ replacement: Any) -> [String: Any] {
    var result = input, approvals = input["approvals"] as! [[String: Any]]
    approvals[0][key] = replacement; result["approvals"] = approvals; return result
}
@MainActor func load(_ app: XCUIApplication, _ count: Int) -> LiveboardFifoContext? {
    app.lbLoadFifo(cycle: 1, expected: Ports.scope${cardinality})
}
@MainActor func heads(_ input: [String: Any], reverse: Bool = false) -> [LiveboardIncoming] {
    var requests = input["requests"] as! [[String: Any]]
    if reverse { requests.reverse() }
    return requests.enumerated().map { index, r in LiveboardIncoming(identifier: "lb-inbox-head-Recover-" + (r["requestId"] as! String), kind: "Recover",
        sender: r["requesterName"] as! String, subject: r["subject"] as! String, count: requests.count - index) }
}
@MainActor func loaderCase(_ input: [String: Any]?, count: Int) -> [String: Any] {
    Ports.reset(input); let app = XCUIApplication(); let loaded = load(app, count)
    return ["accepted": loaded != nil, "loads": Ports.loads, "waits": Ports.waits, "failed": !Ports.failures.isEmpty]
}
@MainActor enum ModePort {
    static var receiver: String?, published = true, plainGrant = false
    static func reset() { receiver = nil; published = true; plainGrant = false }
}
@MainActor func modeCase(_ receiver: String?, published: Bool, plainGrant: Bool) -> [String: Any] {
    ModePort.receiver = receiver; ModePort.published = published; ModePort.plainGrant = plainGrant
    defer { ModePort.reset() }
    return receiverCase(value(1), count: 1)
}
@MainActor func receiverCase(_ input: [String: Any], count: Int, assigned: Bool = true, reverse: Bool = false, absentScope: Bool = false) -> [String: Any] {
    Ports.reset(ModePort.published ? input : nil)
    Ports.expectedPlain = ModePort.plainGrant ? "E2E: IOS_INCOMING_VISIBLE_1_1" : nil
    let app = XCUIApplication(); app.heads = heads(input, reverse: reverse)
    let names = ["android", "web"]
    let senders = Dictionary(uniqueKeysWithValues: (0..<count).map { (names[$0], "subject-\($0)") })
    let approvals = (0..<count).map { Approval(platform: assigned ? "ios" : "android", secret: "subject-\($0)") }
    let harness = ReceiverHarness(app); if absentScope { harness.coordinator = nil }
    harness.run(Cycle(number: 1, senderSecrets: senders, approvals: approvals, offlineReceiver: ModePort.receiver))
    return ["markers": Ports.lines.filter { $0.contains("INCOMING_VISIBLE") }, "grants": Ports.grants,
        "attempts": app.attempts, "decisions": app.decisions, "recover": app.recover, "closes": app.closes,
        "reads": app.reads, "loads": Ports.loads, "failed": !Ports.failures.isEmpty, "events": Ports.events,
        "permits": Ports.permits, "offlineReady": Ports.lines.filter { $0 == "E2E: IOS_OFFLINE_READY_1" }.count,
        "offlineAllowed": Ports.lines.filter { $0 == "E2E: IOS_OFFLINE_STOP_ALLOWED_1" }.count]
}
@MainActor func stepCase(_ name: String) -> [String: Any] {
    var base = value(2)
    if name == "self-head" {
        base = changeRequest(base, 0, "requesterId", "device-ios")
        base = changeRequest(base, 0, "requesterName", "test-ios")
        base = changeRequest(base, 0, "eligiblePeerIds", ["device-android", "device-web"])
    }
    Ports.reset(base); let app = XCUIApplication(); app.heads = heads(base)
    let loaded = load(app, 2)!
    var remaining: Set<Int> = [1, 2], completed: Set<String> = []
    switch name {
    case "foreign-head": app.heads[0] = .init(identifier: "lb-inbox-head-Recover-foreign", kind: "Recover", sender: "test-android", subject: "subject-0", count: 2)
    case "wrong-requester": app.heads[0] = .init(identifier: "lb-inbox-head-Recover-" + ids[0], kind: "Recover", sender: "foreign", subject: "subject-0", count: 2)
    case "wrong-subject": app.heads[0] = .init(identifier: "lb-inbox-head-Recover-" + ids[0], kind: "Recover", sender: "test-android", subject: "foreign", count: 2)
    case "wrong-count": app.heads[0] = .init(identifier: "lb-inbox-head-Recover-" + ids[0], kind: "Recover", sender: "test-android", subject: "subject-0", count: 1)
    case "missing-head": app.readable = false
    case "completed": completed = [ids[0]]
    case "unassigned": remaining = []
    case "wrong-original-step": remaining = [2]
    case "self-head": app.heads[0] = .init(identifier: "lb-inbox-head-Recover-" + ids[0], kind: "Recover", sender: "test-ios", subject: "subject-0", count: 1)
    default: break
    }
    let matched = app.lbFifoStep(context: loaded, completed: completed, remaining: remaining)
    return ["matched": matched != nil, "expansions": app.expansions, "attempts": app.attempts, "decisions": app.decisions]
}
MainActor.assumeIsolated {
    var results: [String: Any] = [:]
    results["loader-two"] = loaderCase(value(2), count: 2)
    results["loader-one"] = loaderCase(value(1), count: 1)
    var opaque = changeRequest(value(2), 0, "requestId", "opaque-current-a")
    opaque = changeRequest(opaque, 1, "requestId", "opaque-current-b")
    results["loader-opaque"] = loaderCase(opaque, count: 2)
    results["receiver-two"] = receiverCase(value(2), count: 2)
    results["receiver-one"] = receiverCase(value(1), count: 1)
    results["receiver-reversed"] = receiverCase(value(2), count: 2, reverse: true)
    results["zero-assigned"] = receiverCase(value(1), count: 1, assigned: false)
    results["missing-scope"] = receiverCase(value(2), count: 2, absentScope: true)
    var invalid: [String: [String: Any]] = [:]
    for key in ["runId", "blockId"] { var v = value(2); v[key] = "foreign"; invalid[key] = v }
    for key in ["testNumber", "cycle"] { var v = value(2); v[key] = 99; invalid[key] = v }
    invalid["duplicate-id"] = changeRequest(value(2), 1, "requestId", ids[0])
    invalid["empty-id"] = changeRequest(value(2), 0, "requestId", "")
    invalid["control-id"] = changeRequest(value(2), 0, "requestId", "unsafe\nvalue")
    invalid["long-id"] = changeRequest(value(2), 0, "requestId", String(repeating: "x", count: 513))
    invalid["empty-pass"] = changeRequest(value(2), 0, "passId", "")
    invalid["empty-subject"] = changeRequest(value(2), 0, "subject", "")
    invalid["unknown-requester"] = changeRequest(value(2), 0, "requesterId", "foreign")
    invalid["requester-name"] = changeRequest(value(2), 0, "requesterName", "foreign")
    invalid["self-eligible"] = changeRequest(value(2), 0, "eligiblePeerIds", ["device-android", "device-ios"])
    invalid["foreign-peer"] = changeRequest(value(2), 0, "eligiblePeerIds", ["foreign", "device-ios"])
    var duplicateRequester = changeRequest(value(2), 1, "requesterId", "device-android")
    duplicateRequester = changeRequest(duplicateRequester, 1, "requesterName", "test-android")
    duplicateRequester = changeRequest(duplicateRequester, 1, "eligiblePeerIds", ["device-web", "device-ios"])
    invalid["duplicate-requester"] = duplicateRequester
    var missingField = value(2), requests = value(2)["requests"] as! [[String: Any]]
    requests[0].removeValue(forKey: "requestId"); missingField["requests"] = requests; invalid["missing-id"] = missingField
    for mode in ["missing-member", "duplicate-member", "empty-member"] {
        var v = value(2), members = v["identities"] as! [String: [String: String]]
        if mode == "missing-member" { members.removeValue(forKey: "ios") }
        if mode == "duplicate-member" { members["ios"] = members["android"] }
        if mode == "empty-member" { members["ios"]?["deviceName"] = "" }
        v["identities"] = members; invalid[mode] = v
    }
    for (name, input) in invalid { results["invalid:" + name] = loaderCase(input, count: 2) }
    results["invalid:expected-zero"] = loaderCase(value(2), count: 0)
    results["invalid:expected-three"] = loaderCase(value(3), count: 3)
    results["invalid:count-one-two"] = loaderCase(value(2), count: 1)
    results["invalid:count-two-one"] = loaderCase(value(1), count: 2)
    results["invalid:zero-actual"] = loaderCase(value(0), count: 1)
    results["invalid:three-actual"] = loaderCase(value(3), count: 2)
    results["invalid:no-data"] = loaderCase(nil, count: 2)
    for name in ["foreign-head", "wrong-requester", "wrong-subject", "wrong-count", "missing-head", "completed", "unassigned", "wrong-original-step", "self-head"] {
        results["step:" + name] = stepCase(name)
    }
    for (name, input) in ["wrong-pass": changeApproval(value(2), "passId", "foreign"),
        "wrong-subject": changeApproval(value(2), "subject", "foreign"),
        "wrong-platform": changeApproval(value(2), "approverPlatform", "android"),
        "opposite-action": changeApproval(value(2), "action", "decline")] {
        results["receiver-invalid:" + name] = receiverCase(input, count: 2)
    }
    results["mode:offline-android"] = modeCase("android", published: false, plainGrant: true)
    results["mode:offline-web"] = modeCase("web", published: false, plainGrant: true)
    results["mode:offline-ios"] = modeCase("ios", published: false, plainGrant: false)
    results["mode:generic-unpublished"] = modeCase(nil, published: false, plainGrant: false)
    let data = try! JSONSerialization.data(withJSONObject: results, options: [.sortedKeys])
    Swift.print("SINGLE_FIFO_FIXTURE " + String(decoding: data, as: UTF8.self))
}
`;
}

export function runSingleFifoFixture(inbox, caller, observation) {
  const root = mkdtempSync(resolve(tmpdir(), 'ms-r32-single-fifo-')); mkdirSync(resolve(root, 'cache'));
  try {
    const path = resolve(root, 'fixture.swift'); writeFileSync(path, buildSingleFifoFixture(inbox, caller, observation));
    const output = execFileSync('/usr/bin/swift', ['-module-cache-path', resolve(root, 'cache'), path], { encoding: 'utf8', timeout: 25000 });
    const line = output.split('\n').find(value => value.startsWith('SINGLE_FIFO_FIXTURE '));
    assert(line, 'Whole real Swift loader, step and receiver body must finish before semantic assertions');
    return JSON.parse(line.slice('SINGLE_FIFO_FIXTURE '.length));
  } finally { rmSync(root, { recursive: true, force: true }); }
}
