import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
const source = readFileSync(new URL('../../../meta-secret-compose/iosApp/iosAppUITests/CaseEightIosBothOfflineUITest.swift', import.meta.url), 'utf8');

test('iOS search guarantees independent up/down budgets and absence requires both proven ends', () => {
  const search = source.slice(source.indexOf('    private func test50FindInList('), source.indexOf('    nonisolated private static func test50ViewportKey('));
  assert(search.includes('for upward in [true, false]'));
  assert(search.indexOf('let directionDeadline = Date().addingTimeInterval(60)') > search.indexOf('for upward in [true, false]'));
  assert(search.includes('for _ in 0..<64'));
  assert(search.includes('unchanged >= 3'));
  assert(search.includes('reachedBothEnds = reachedBothEnds && unchanged >= 3'));
  assert(search.includes('XCTAssertTrue(reachedBothEnds'));
  assert(!search.includes('map { "\\($0.identifier):\\($0.frame)" }'));
});

test('actual Swift viewport key ignores elastic bounce while retaining clipping and identity changes', { timeout: 30000 }, () => {
  const start = source.indexOf('    nonisolated private static func test50ViewportKey(');
  const end = source.indexOf('    private func test50RequireVisible(', start);
  assert(start >= 0 && end > start);
  const method = source.slice(start, end).replace('nonisolated private static func', 'static func');
  const root = mkdtempSync(resolve(tmpdir(), 'ms96-viewport-regression-')); mkdirSync(resolve(root, 'cache'));
  try {
    const fixture = `import Foundation\nimport CoreGraphics\nstruct Probe {\n${method}\n}\nlet viewport = CGRect(x: 0, y: 0, width: 200, height: 400)\nlet row = CGRect(x: 0, y: 0, width: 200, height: 100)\nlet base = Probe.test50ViewportKey(id: "row", row: row, viewport: viewport)\nfor delta in [-3.0, -1.0, 0.0, 1.0, 3.0] { precondition(Probe.test50ViewportKey(id: "row", row: row.offsetBy(dx: 0, dy: delta), viewport: viewport) == base) }\nprecondition(Probe.test50ViewportKey(id: "other-row", row: row, viewport: viewport) != base)\nprecondition(Probe.test50ViewportKey(id: "row", row: row.offsetBy(dx: 0, dy: -30), viewport: viewport) != base)\nprint("viewport-key-regression-pass")\n`;
    const path = resolve(root, 'fixture.swift'); writeFileSync(path, fixture);
    const output = execFileSync('/usr/bin/swift', ['-module-cache-path', resolve(root, 'cache'), path], { encoding: 'utf8', timeout: 25000 });
    assert(output.includes('viewport-key-regression-pass'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});
