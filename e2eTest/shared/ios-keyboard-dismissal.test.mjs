import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';

// The private RED and public GREEN both consume the actual current Swift owner.
// Root binds this e2eTest cwd/source and separately admits this host-only command.
const swiftPath = resolve(process.cwd(), '../../meta-secret-compose/iosApp/iosAppUITests/LiveboardTestActions.swift');
const source = readFileSync(swiftPath, 'utf8');
const start = source.indexOf('    func lbDismissKeyboard() {');
const end = source.indexOf('\n    }\n', start) + '\n    }\n'.length;
assert(start >= 0 && end > start, 'Actual Swift dismissal declaration is missing');
const method = source.slice(start, end);
let receipts;

function actualSwiftReceipts() {
  if (receipts) return receipts;
  const directory = mkdtempSync(resolve(tmpdir(), 'MS-20261019210543-r20-keyboard-'));
  mkdirSync(resolve(directory, 'cache'), { mode: 0o700 });
  try {
    // Only UIKit/XCTest-surface doubles are supplied; the decision body is exact source.
    const fixture = String.raw`import Foundation
struct CGVector { let dx: Double; let dy: Double }
enum ElementType { case any }
final class State {
    var keyboard: Bool
    let offered: Set<String>
    let disabled: Set<String>
    let title: Bool
    let original = String(repeating: "x", count: 3)
    var text = String(repeating: "x", count: 3)
    var returnTaps = 0, doneTaps = 0, outsideTaps = 0, buttonQueries = 0
    init(_ keyboard: Bool, _ offered: Set<String>, _ disabled: Set<String>, _ title: Bool) {
        self.keyboard = keyboard; self.offered = offered; self.disabled = disabled; self.title = title
    }
}
struct Button {
    let state: State; let name: String
    var exists: Bool { state.offered.contains(name) }
    var isHittable: Bool { !state.disabled.contains(name) }
    func tap() {
        if name == "Return" { state.returnTaps += 1; state.text += "\n" }
        else { state.doneTaps += 1; state.keyboard = false }
    }
}
struct Buttons {
    let state: State
    subscript(_ name: String) -> Button { state.buttonQueries += 1; return Button(state: state, name: name) }
}
struct Keyboard { let state: State; var exists: Bool { state.keyboard } }
struct Keyboards {
    let state: State
    var firstMatch: Keyboard { Keyboard(state: state) }
    var buttons: Buttons { Buttons(state: state) }
}
struct Element { let exists: Bool }
struct Elements {
    let state: State
    subscript(_ identifier: String) -> Element { Element(exists: state.title && identifier == "lb-create-name") }
}
struct Coordinate { let state: State; func tap() { state.outsideTaps += 1 } }
final class App {
    let state: State
    init(_ state: State) { self.state = state }
    var keyboards: Keyboards { Keyboards(state: state) }
    func descendants(matching: ElementType) -> Elements { Elements(state: state) }
    func coordinate(withNormalizedOffset: CGVector) -> Coordinate { Coordinate(state: state) }
${method}
}
struct Receipt: Codable {
    let textUnchanged: Bool
    let returnTaps: Int, doneTaps: Int, outsideTaps: Int, buttonQueries: Int
    let keyboardPresent: Bool
}
let cases = [
    State(true, ["Return"], [], false),
    State(true, [], [], true),
    State(true, ["Done", "Return"], [], true),
    State(true, ["Готово", "Return"], [], true),
    State(true, ["Done", "Return"], ["Done"], true),
    State(false, ["Done", "Return"], [], true)
]
let results = cases.map { state in
    let app = App(state)
    app.lbDismissKeyboard()
    app.lbDismissKeyboard()
    return Receipt(textUnchanged: state.text == state.original, returnTaps: state.returnTaps,
        doneTaps: state.doneTaps, outsideTaps: state.outsideTaps, buttonQueries: state.buttonQueries,
        keyboardPresent: state.keyboard)
}
let data = try JSONEncoder().encode(results)
print(String(data: data, encoding: .utf8)!)
`;
    const path = resolve(directory, 'fixture.swift');
    writeFileSync(path, fixture, { mode: 0o600 });
    const child = spawnSync('/usr/bin/swift', ['-module-cache-path', resolve(directory, 'cache'), path], {
      encoding: 'utf8', timeout: 25_000, maxBuffer: 64 * 1024,
      env: { ...process.env, TMPDIR: directory },
    });
    const naturalExit0 = child.error === undefined && child.signal === null && child.status === 0;
    assert.equal(naturalExit0, true, 'Host Swift fixture must exit naturally0 before behavioral assertions');
    const decoded = JSON.parse(child.stdout);
    assert.equal(Array.isArray(decoded) && decoded.length === 6, true, 'Exact fixed host fixture variants required');
    for (const row of decoded) {
      assert.deepEqual(Object.keys(row).sort(), ['buttonQueries', 'doneTaps', 'keyboardPresent', 'outsideTaps', 'returnTaps', 'textUnchanged']);
      assert.equal(typeof row.textUnchanged === 'boolean' && typeof row.keyboardPresent === 'boolean', true);
      for (const key of ['returnTaps', 'doneTaps', 'outsideTaps', 'buttonQueries']) {
        assert.equal(Number.isInteger(row[key]) && row[key] >= 0 && row[key] <= 6, true);
      }
    }
    receipts = decoded;
    console.log('LB_KEYBOARD_HOST_FIXTURE ' + JSON.stringify({ version: 1, naturalExit0, variants: decoded.length }));
    return receipts;
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test('actual Swift dismissal preserves exact typed text when only Return is offered', { timeout: 30_000 }, () => {
  const row = actualSwiftReceipts()[0];
  assert.equal(row.textUnchanged, true, 'Keyboard dismissal must preserve exact typed text when only Return is offered');
  assert.equal(row.returnTaps, 0);
  assert.equal(row.outsideTaps, 0);
});

test('actual Swift dismissal never substitutes an outside coordinate when no Done is offered', () => {
  const row = actualSwiftReceipts()[1];
  assert.equal(row.textUnchanged, true);
  assert.equal(row.outsideTaps, 0);
  assert.equal(row.keyboardPresent, true);
});

test('actual Swift dismissal retains offered hittable Done and Gotovo semantics', () => {
  for (const row of actualSwiftReceipts().slice(2, 4)) {
    assert.equal(row.textUnchanged, true);
    assert.equal(row.doneTaps, 1);
    assert.equal(row.returnTaps, 0);
    assert.equal(row.outsideTaps, 0);
    assert.equal(row.keyboardPresent, false);
  }
});

test('actual Swift dismissal leaves an inaccessible Done keyboard to the real target action', () => {
  const row = actualSwiftReceipts()[4];
  assert.equal(row.textUnchanged, true);
  assert.equal(row.doneTaps, 0);
  assert.equal(row.returnTaps, 0);
  assert.equal(row.outsideTaps, 0);
  assert.equal(row.keyboardPresent, true);
});

test('actual Swift dismissal makes no button query or gesture when keyboard is absent', () => {
  const row = actualSwiftReceipts()[5];
  assert.equal(row.textUnchanged, true);
  assert.equal(row.buttonQueries, 0);
  assert.equal(row.doneTaps + row.returnTaps + row.outsideTaps, 0);
});
